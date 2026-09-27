//! Deterministic unit tests for the live-mixer wiring (no audio devices):
//! de-interleave, stereo → device mapping, key dispatch and clamping,
//! voice-chain exclusivity, scene slugs and files, the `MixerState` and
//! `mixer-meters` JSON shapes, and the mono level backwards-compat rule.
use crate::dsp::mixer::{BusMeters, Mixer, StripMeters, BUS_MAIN, BUS_MONITOR, BUS_STREAM};
use crate::dsp::test_util::{rms, sine};
use crate::mixer_control::{
    scene_slug, MixerControl, StoredScene, MAX_STRIPS, OTHER_STRIPS_FADER_DB,
};
use crate::mixer_meters::{MixerMeterSlots, METER_FLOOR_DB};
use crate::routing::{deinterleave, device_sample, push_stereo};
use ringbuf::traits::{Consumer, Split};
use std::collections::BTreeSet;
use std::sync::atomic::Ordering::Relaxed;

fn keys(value: &serde_json::Value) -> BTreeSet<String> {
    value
        .as_object()
        .expect("JSON object")
        .keys()
        .cloned()
        .collect()
}

fn set(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| s.to_string()).collect()
}

// ── De-interleave ───────────────────────────────────────────────────────────

#[test]
fn deinterleave_splits_channels_into_buffers() {
    // 3 frames of 3 channels.
    let data = [1.0f32, 2.0, 3.0, 11.0, 12.0, 13.0, 21.0, 22.0, 23.0];
    let mut outs = vec![vec![0.0f32; 8]; 3];
    let frames = deinterleave(&data, 3, &mut outs, |s| s);
    assert_eq!(frames, 3);
    assert_eq!(&outs[0][..3], &[1.0, 11.0, 21.0]);
    assert_eq!(&outs[1][..3], &[2.0, 12.0, 22.0]);
    assert_eq!(&outs[2][..3], &[3.0, 13.0, 23.0]);
}

#[test]
fn deinterleave_skips_extra_channels_and_respects_capacity() {
    let data = [1i16, 2, 3, 4, 5, 6, 7, 8];
    // Only two buffers for a 4-channel stream; each holds one frame.
    let mut outs = vec![vec![0.0f32; 1]; 2];
    let frames = deinterleave(&data, 4, &mut outs, |s| s as f32);
    assert_eq!(frames, 1);
    assert_eq!(outs[0][0], 1.0);
    assert_eq!(outs[1][0], 2.0);
    // A partial trailing frame is ignored; mono needs no special case.
    let mut mono = vec![vec![0.0f32; 4]];
    assert_eq!(deinterleave(&[0.5f32, -0.5, 0.25], 1, &mut mono, |s| s), 3);
    assert_eq!(&mono[0][..3], &[0.5, -0.5, 0.25]);
    assert_eq!(deinterleave(&[1.0f32, 2.0, 3.0], 2, &mut mono, |s| s), 1);
}

// ── Stereo bus → device channels ────────────────────────────────────────────

fn assert_close(got: &[f32], want: &[f32]) {
    assert_eq!(got.len(), want.len(), "{got:?} vs {want:?}");
    for (g, w) in got.iter().zip(want.iter()) {
        assert!((g - w).abs() < 1e-6, "{got:?} vs {want:?}");
    }
}

#[test]
fn stereo_maps_to_mono_stereo_and_multichannel_devices() {
    let one: Vec<f32> = (0..1).map(|ch| device_sample(0.2, 0.6, 1, ch)).collect();
    assert_close(&one, &[0.4]);
    let two: Vec<f32> = (0..2).map(|ch| device_sample(0.2, 0.6, 2, ch)).collect();
    assert_close(&two, &[0.2, 0.6]);
    let six: Vec<f32> = (0..6).map(|ch| device_sample(0.2, 0.6, 6, ch)).collect();
    assert_close(&six, &[0.2, 0.6, 0.0, 0.0, 0.0, 0.0]);
}

fn push_and_collect(channels: usize, gain: f32) -> Vec<f32> {
    let (mut prod, mut cons) = ringbuf::HeapRb::<f32>::new(64).split();
    let dropped = push_stereo(&mut prod, &[0.2, 0.4], &[0.6, 0.8], channels, gain);
    assert_eq!(dropped, 0);
    let mut out = Vec::new();
    while let Some(v) = cons.try_pop() {
        out.push(v);
    }
    out
}

#[test]
fn push_stereo_interleaves_per_device_layout() {
    assert_close(&push_and_collect(1, 1.0), &[0.4, 0.6]);
    assert_close(&push_and_collect(2, 1.0), &[0.2, 0.6, 0.4, 0.8]);
    assert_close(
        &push_and_collect(6, 1.0),
        &[0.2, 0.6, 0.0, 0.0, 0.0, 0.0, 0.4, 0.8, 0.0, 0.0, 0.0, 0.0],
    );
    assert_close(&push_and_collect(2, 0.5), &[0.1, 0.3, 0.2, 0.4]);
}

#[test]
fn push_stereo_counts_dropped_samples() {
    let (mut prod, _cons) = ringbuf::HeapRb::<f32>::new(4).split();
    let dropped = push_stereo(&mut prod, &[0.1, 0.2], &[0.1, 0.2], 6, 1.0);
    assert_eq!(dropped, 12 - 4);
}

// ── Key dispatch, clamping, errors ──────────────────────────────────────────

#[test]
fn strip_keys_dispatch_and_clamp() {
    let m = MixerControl::new();
    let s = &m.params.strips[2];
    m.set_strip_param(2, "trim_db", 100.0).unwrap();
    assert_eq!(s.trim_db.load(Relaxed), 40.0);
    m.set_strip_param(2, "hpf_hz", 5.0).unwrap();
    assert_eq!(s.hpf_freq_hz.load(Relaxed), 20.0);
    m.set_strip_param(2, "gate_threshold_db", -200.0).unwrap();
    assert_eq!(s.gate_threshold_db.load(Relaxed), -80.0);
    m.set_strip_param(2, "comp_ratio", 0.0).unwrap();
    assert_eq!(s.comp_ratio.load(Relaxed), 1.0);
    m.set_strip_param(2, "pan", -3.0).unwrap();
    assert_eq!(s.pan.load(Relaxed), -1.0);
    m.set_strip_param(2, "send_monitor_db", 50.0).unwrap();
    assert_eq!(s.send_db[BUS_MONITOR].load(Relaxed), 10.0);
    m.set_strip_param(2, "send_stream_db", -6.0).unwrap();
    assert_eq!(s.send_db[BUS_STREAM].load(Relaxed), -6.0);
    m.set_strip_param(2, "send_main_db", -500.0).unwrap();
    assert_eq!(s.send_db[BUS_MAIN].load(Relaxed), -90.0);
    // NaN falls back to the key's default.
    m.set_strip_param(2, "fader_db", f32::NAN).unwrap();
    assert_eq!(s.fader_db.load(Relaxed), 0.0);

    m.set_strip_bool(2, "polarity", true).unwrap();
    assert!(s.polarity_invert.load(Relaxed));
    m.set_strip_bool(2, "hpf_enabled", false).unwrap();
    assert!(!s.hpf_enabled.load(Relaxed));

    assert!(m.set_strip_param(2, "volume", 1.0).is_err());
    assert!(m.set_strip_param(2, "mute", 1.0).is_err());
    assert!(m.set_strip_bool(2, "trim_db", true).is_err());
    assert!(m.set_strip_param(MAX_STRIPS as u32, "trim_db", 0.0).is_err());
    assert!(m.set_strip_bool(99, "mute", true).is_err());
    assert!(m.rename_strip(MAX_STRIPS as u32, "x").is_err());
}

#[test]
fn bus_keys_dispatch_and_clamp() {
    let m = MixerControl::new();
    m.set_bus_param("stream", "limiter_ceiling_db", -50.0).unwrap();
    assert_eq!(m.params.buses[BUS_STREAM].limiter_ceiling_db.load(Relaxed), -12.0);
    m.set_bus_param("main", "fader_db", 20.0).unwrap();
    assert_eq!(m.params.buses[BUS_MAIN].fader_db.load(Relaxed), 10.0);
    m.set_bus_bool("monitor", "mute", true).unwrap();
    assert!(m.params.buses[BUS_MONITOR].mute.load(Relaxed));

    assert!(m.set_bus_param("aux", "fader_db", 0.0).is_err());
    assert!(m.set_bus_param("main", "pan", 0.0).is_err());
    assert!(m.set_bus_bool("main", "solo", true).is_err());
    assert!(m.set_bus_bool("Main", "mute", true).is_err());
}

#[test]
fn rename_trims_bounds_and_defaults() {
    let m = MixerControl::new();
    m.rename_strip(1, "  Pastor  ").unwrap();
    assert!(m.rename_strip(1, &"x".repeat(25)).is_err());
    m.rename_strip(2, &"é".repeat(24)).unwrap();
    m.rename_strip(3, "   ").unwrap();
    let state = m.snapshot(4, false, Vec::new());
    assert_eq!(state.strips[1].name, "Pastor");
    assert_eq!(state.strips[2].name.chars().count(), 24);
    assert_eq!(state.strips[3].name, "Ch 4");
}

#[test]
fn only_the_first_strip_fader_is_up_by_default() {
    let m = MixerControl::new();
    let state = m.snapshot(MAX_STRIPS as u32, false, Vec::new());
    assert_eq!(state.strips[0].fader_db, 0.0);
    for strip in &state.strips[1..] {
        assert_eq!(strip.fader_db, OTHER_STRIPS_FADER_DB, "strip {}", strip.index);
    }
    let v = serde_json::to_value(m.snapshot(2, false, Vec::new())).unwrap();
    assert_eq!(v["strips"][1]["fader_db"], -90.0);

    // A scene that only covers strip 0 resets the others to these defaults.
    m.set_strip_param(3, "fader_db", 0.0).unwrap();
    let mut scene = MixerControl::new().capture_scene("Short");
    scene.mixer.strips.truncate(1);
    scene.mixer.strips[0].fader_db = f32::NAN;
    m.apply_scene(&scene);
    let after = m.snapshot(4, false, Vec::new());
    assert_eq!(after.strips[0].fader_db, 0.0);
    assert_eq!(after.strips[3].fader_db, OTHER_STRIPS_FADER_DB);
}

// ── Voice chain exclusivity ─────────────────────────────────────────────────

fn voice_flags(m: &MixerControl) -> Vec<bool> {
    m.snapshot(8, false, Vec::new())
        .strips
        .iter()
        .map(|s| s.voice_chain)
        .collect()
}

#[test]
fn voice_chain_is_exclusive() {
    let m = MixerControl::new();
    assert_eq!(voice_flags(&m), vec![true, false, false, false, false, false, false, false]);

    m.set_strip_bool(3, "voice_chain", true).unwrap();
    assert_eq!(m.voice_strip.load(Relaxed), 3);
    let flags = voice_flags(&m);
    assert_eq!(flags.iter().filter(|&&on| on).count(), 1);
    assert!(flags[3] && !flags[0]);

    // Turning it off elsewhere is a no-op; off on the holder is refused.
    m.set_strip_bool(0, "voice_chain", false).unwrap();
    assert_eq!(m.voice_strip.load(Relaxed), 3);
    assert!(m.set_strip_bool(3, "voice_chain", false).is_err());
    assert_eq!(m.voice_strip.load(Relaxed), 3);
    assert!(m.set_strip_bool(40, "voice_chain", true).is_err());
    assert_eq!(m.voice_strip.load(Relaxed), 3);
}

// ── Scene slugs and files ───────────────────────────────────────────────────

#[test]
fn scene_slug_accepts_contract_names() {
    assert_eq!(scene_slug("Sermon").unwrap(), "sermon");
    assert_eq!(scene_slug("Sunday Worship 2").unwrap(), "sunday-worship-2");
    assert_eq!(scene_slug("youth_band-A").unwrap(), "youth_band-a");
    assert_eq!(scene_slug("  Padded  ").unwrap(), "padded");
    assert_eq!(scene_slug(&"a".repeat(40)).unwrap(), "a".repeat(40));
}

#[test]
fn scene_slug_rejects_unsafe_names() {
    for bad in [
        "", "   ", "../x", "..", "a/b", "a\\b", "C:x", "x.json", "name\0", "tab\tname",
        "Café", "礼拜", "emoji🎵", "con", "NUL", "com1", "LPT9",
    ] {
        assert!(scene_slug(bad).is_err(), "should reject {bad:?}");
    }
    assert!(scene_slug(&"a".repeat(41)).is_err());
    assert!(scene_slug("com10").is_ok());
}

#[test]
fn scene_round_trip_restores_and_clamps() {
    let m = MixerControl::new();
    m.set_strip_param(1, "fader_db", -12.0).unwrap();
    m.set_strip_bool(1, "voice_chain", true).unwrap();
    m.rename_strip(1, "Pulpit").unwrap();
    m.set_bus_param("monitor", "fader_db", -3.0).unwrap();

    let json = serde_json::to_string(&m.capture_scene("Sermon")).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["name"], "Sermon");
    // Tamper with the file: an out-of-range trim must be clamped on load.
    value["mixer"]["strips"][0]["trim_db"] = serde_json::json!(999.0);
    let scene: StoredScene = serde_json::from_value(value).unwrap();

    let fresh = MixerControl::new();
    fresh.apply_scene(&scene);
    let state = fresh.snapshot(2, false, Vec::new());
    assert_eq!(state.strips[1].fader_db, -12.0);
    assert_eq!(state.strips[1].name, "Pulpit");
    assert!(state.strips[1].voice_chain && !state.strips[0].voice_chain);
    assert_eq!(state.strips[0].trim_db, 40.0);
    assert_eq!(state.buses[BUS_MONITOR].fader_db, -3.0);
}

// ── JSON shapes ─────────────────────────────────────────────────────────────

#[test]
fn mixer_state_json_shape_matches_contract() {
    let m = MixerControl::new();
    let state = m.snapshot(2, true, vec!["Sermon".to_string()]);
    let v = serde_json::to_value(&state).unwrap();

    assert_eq!(
        keys(&v),
        set(&["inputChannels", "running", "strips", "buses", "scenes"])
    );
    assert_eq!(v["inputChannels"], 2);
    assert_eq!(v["running"], true);
    assert_eq!(v["scenes"], serde_json::json!(["Sermon"]));

    let strips = v["strips"].as_array().unwrap();
    assert_eq!(strips.len(), 2);
    assert_eq!(
        keys(&strips[0]),
        set(&[
            "index", "name", "trim_db", "polarity", "hpf_enabled", "hpf_hz", "gate_enabled",
            "gate_threshold_db", "eq_low_db", "eq_mid_db", "eq_high_db", "comp_enabled",
            "comp_threshold_db", "comp_ratio", "pan", "mute", "solo", "fader_db",
            "send_main_db", "send_stream_db", "send_monitor_db", "monitor_post_fader",
            "voice_chain",
        ])
    );
    // Contract defaults.
    let s0 = &strips[0];
    assert_eq!(s0["index"], 0);
    assert_eq!(s0["name"], "Ch 1");
    assert_eq!(s0["hpf_enabled"], true);
    assert_eq!(s0["hpf_hz"], 80.0);
    assert_eq!(s0["gate_threshold_db"], -45.0);
    assert_eq!(s0["comp_threshold_db"], -18.0);
    assert_eq!(s0["comp_ratio"], 3.0);
    assert_eq!(s0["send_monitor_db"], 0.0);
    assert_eq!(s0["monitor_post_fader"], false);
    assert_eq!(s0["voice_chain"], true);
    assert_eq!(strips[1]["voice_chain"], false);

    let buses = v["buses"].as_array().unwrap();
    let ids: Vec<&str> = buses.iter().map(|b| b["id"].as_str().unwrap()).collect();
    assert_eq!(ids, vec!["main", "stream", "monitor"]);
    assert_eq!(keys(&buses[0]), set(&["id", "fader_db", "mute", "limiter_ceiling_db"]));
    assert_eq!(buses[2]["limiter_ceiling_db"], -1.0);

    // Stopped: one channel, strips capped by the channel count.
    let stopped = serde_json::to_value(m.snapshot(1, false, Vec::new())).unwrap();
    assert_eq!(stopped["inputChannels"], 1);
    assert_eq!(stopped["running"], false);
    assert_eq!(stopped["strips"].as_array().unwrap().len(), 1);
    let many = m.snapshot(64, true, Vec::new());
    assert_eq!(many.strips.len(), MAX_STRIPS);
}

#[test]
fn mixer_meters_json_shape_and_clamping() {
    let slots = MixerMeterSlots::new(2);
    let strips = [
        StripMeters {
            pre_fader_peak_db: -30.0,
            post_fader_peak_db: -33.0,
            gate_open: true,
            gain_reduction_db: -2.5,
        },
        StripMeters {
            pre_fader_peak_db: -120.0,
            post_fader_peak_db: -120.0,
            gate_open: false,
            gain_reduction_db: 0.0,
        },
    ];
    let bus = BusMeters { peak_l_db: -12.0, peak_r_db: -14.0, rms_l_db: -20.0, rms_r_db: -130.0 };
    slots.publish(&strips, &[bus, bus, bus]);
    // A quieter later block does not lower the held peak.
    let quieter = StripMeters { pre_fader_peak_db: -50.0, ..strips[0] };
    slots.publish(&[quieter], &[]);

    let v = serde_json::to_value(slots.take_payload()).unwrap();
    assert_eq!(keys(&v), set(&["strips", "buses"]));
    let s = v["strips"].as_array().unwrap();
    assert_eq!(s.len(), 2);
    assert_eq!(keys(&s[0]), set(&["pre_db", "post_db", "gate_open", "gr_db"]));
    assert_eq!(s[0]["pre_db"], -30.0);
    assert_eq!(s[0]["post_db"], -33.0);
    assert_eq!(s[0]["gate_open"], true);
    assert_eq!(s[0]["gr_db"], 2.5);
    assert_eq!(s[1]["pre_db"], METER_FLOOR_DB);

    let b = v["buses"].as_array().unwrap();
    assert_eq!(b.len(), 3);
    assert_eq!(
        keys(&b[0]),
        set(&["id", "peak_l_db", "peak_r_db", "rms_l_db", "rms_r_db"])
    );
    assert_eq!(b[0]["id"], "main");
    assert_eq!(b[2]["id"], "monitor");
    assert_eq!(b[1]["peak_r_db"], -14.0);
    assert_eq!(b[1]["rms_r_db"], METER_FLOOR_DB);

    // Held peaks reset after a read.
    let again = slots.take_payload();
    assert_eq!(again.strips[0].pre_db, METER_FLOOR_DB);
    assert_eq!(again.strips[0].gr_db, 0.0);
}

// ── Mono backwards compatibility ────────────────────────────────────────────

/// With one input channel and the contract defaults, a centred mono strip
/// must reach Stream and Monitor at the level of the old mono path, which
/// pushed the voice unchanged to every channel of every output device.
#[test]
fn centred_mono_strip_keeps_the_old_level_on_stream_and_monitor() {
    const SR: usize = 48_000;
    const BLOCK: usize = 256;
    let control = MixerControl::new();
    let mut mixer = Mixer::new(SR as f64, BLOCK, 1, control.params.clone());
    // 1 kHz is far above the default 80 Hz HPF; 0.5 is below the limiter.
    let input = sine(SR, 1_000.0, SR as f32, 0.5);

    let mut outs: Vec<(Vec<f32>, Vec<f32>)> = vec![(Vec::new(), Vec::new()); 3];
    for chunk in input.chunks(BLOCK) {
        let n = mixer.process(&[chunk], chunk.len());
        assert_eq!(n, chunk.len());
        for (bus, out) in outs.iter_mut().enumerate() {
            let (l, r) = mixer.output(bus);
            out.0.extend_from_slice(l);
            out.1.extend_from_slice(r);
        }
    }

    let tail = SR / 2;
    let want = rms(&input[input.len() - tail..]);
    for bus in [BUS_STREAM, BUS_MONITOR, BUS_MAIN] {
        let (l, r) = &outs[bus];
        let (l, r) = (&l[l.len() - tail..], &r[r.len() - tail..]);
        let mono: Vec<f32> = l
            .iter()
            .zip(r.iter())
            .map(|(&a, &b)| device_sample(a, b, 1, 0))
            .collect();
        for (label, got) in [("left", rms(l)), ("right", rms(r)), ("mono", rms(&mono))] {
            let ratio = got / want;
            assert!((ratio - 1.0).abs() < 0.01, "bus {bus} {label}: ratio {ratio}");
        }
    }
}

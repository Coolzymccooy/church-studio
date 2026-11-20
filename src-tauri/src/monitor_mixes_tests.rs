//! Control side of mon 2 / mon 3 (`docs/specs/2026-11-20-monitor-mixes.md`):
//! the new strip and bus keys, `mixer_state` buses with `output`, the
//! five-bus `mixer-meters` payload and old scene files.
use crate::dsp::mixer::{BusMeters, BUS_MONITOR, BUS_MONITOR2, BUS_MONITOR3, NUM_BUSES};
use crate::mixer_control::{BusOutputs, MixerControl, StoredScene, BUS_IDS};
use crate::mixer_layout::StripLayout;
use crate::mixer_meters::MixerMeterSlots;
use std::sync::atomic::Ordering::Relaxed;

#[test]
fn new_sends_default_off_and_clamp_like_the_others() {
    let m = MixerControl::new();
    let s = &m.params.strips[0];
    assert_eq!(s.send_db[BUS_MONITOR2].load(Relaxed), -90.0);
    assert_eq!(s.send_db[BUS_MONITOR3].load(Relaxed), -90.0);

    m.set_strip_param(0, "send_monitor2_db", 50.0).unwrap();
    assert_eq!(s.send_db[BUS_MONITOR2].load(Relaxed), 10.0);
    m.set_strip_param(0, "send_monitor3_db", -500.0).unwrap();
    assert_eq!(s.send_db[BUS_MONITOR3].load(Relaxed), -90.0);
    m.set_strip_param(0, "send_monitor3_db", f32::NAN).unwrap();
    assert_eq!(s.send_db[BUS_MONITOR3].load(Relaxed), -90.0, "NaN → default (off)");
    m.set_strip_param(0, "send_monitor2_db", -12.0).unwrap();

    let state = m.snapshot(1, false, Vec::new());
    assert_eq!(state.strips[0].send_monitor2_db, -12.0);
    assert_eq!(state.strips[0].send_monitor3_db, -90.0);
    // Mon 1 is unchanged.
    assert_eq!(state.strips[0].send_monitor_db, 0.0);
    assert_eq!(s.send_db[BUS_MONITOR].load(Relaxed), 0.0);
}

#[test]
fn mon2_and_mon3_take_the_bus_keys() {
    let m = MixerControl::new();
    m.set_bus_param("monitor2", "fader_db", -4.0).unwrap();
    m.set_bus_param("monitor3", "limiter_ceiling_db", -50.0).unwrap();
    m.set_bus_bool("monitor3", "mute", true).unwrap();
    assert_eq!(m.params.buses[BUS_MONITOR2].fader_db.load(Relaxed), -4.0);
    assert_eq!(m.params.buses[BUS_MONITOR3].limiter_ceiling_db.load(Relaxed), -12.0);
    assert!(m.params.buses[BUS_MONITOR3].mute.load(Relaxed));
    let err = m.set_bus_param("monitor4", "fader_db", 0.0).unwrap_err();
    assert!(err.contains("monitor2") && err.contains("monitor3"), "{err}");
}

#[test]
fn mixer_state_lists_five_buses_with_their_output() {
    let m = MixerControl::new();
    let mut outputs = BusOutputs::default();
    outputs[BUS_MONITOR] = Some("Desk phones".to_string());
    outputs[BUS_MONITOR3] = Some("Worship IEM".to_string());
    let state = m.snapshot_outputs(StripLayout::new(1, false, 0), true, Vec::new(), &outputs);
    let v = serde_json::to_value(&state).unwrap();
    let buses = v["buses"].as_array().unwrap();
    assert_eq!(buses.len(), 5);
    let ids: Vec<&str> = buses.iter().map(|b| b["id"].as_str().unwrap()).collect();
    assert_eq!(ids, BUS_IDS.to_vec());
    assert_eq!(ids, vec!["main", "stream", "monitor", "monitor2", "monitor3"]);
    assert_eq!(buses[0]["output"], serde_json::Value::Null);
    assert_eq!(buses[2]["output"], "Desk phones");
    assert_eq!(buses[3]["output"], serde_json::Value::Null);
    assert_eq!(buses[4]["output"], "Worship IEM");
    for bus in &buses[3..] {
        assert_eq!(bus["fader_db"], 0.0);
        assert_eq!(bus["limiter_ceiling_db"], -1.0);
        assert_eq!(bus["mute"], false);
    }

    // Stopped (no outputs): every output is null.
    let stopped = serde_json::to_value(m.snapshot(1, false, Vec::new())).unwrap();
    for bus in stopped["buses"].as_array().unwrap() {
        assert_eq!(bus["output"], serde_json::Value::Null);
    }
}

#[test]
fn mixer_meters_carry_five_buses() {
    let slots = MixerMeterSlots::new(1);
    let quiet = BusMeters {
        peak_l_db: -40.0,
        peak_r_db: -40.0,
        rms_l_db: -50.0,
        rms_r_db: -50.0,
        leveler_db: 0.0,
    };
    let mut buses = [quiet; NUM_BUSES];
    buses[BUS_MONITOR3].peak_l_db = -6.0;
    slots.publish(&[], &buses);
    let v = serde_json::to_value(slots.take_payload()).unwrap();
    let b = v["buses"].as_array().unwrap();
    assert_eq!(b.len(), 5);
    let ids: Vec<&str> = b.iter().map(|x| x["id"].as_str().unwrap()).collect();
    assert_eq!(ids, vec!["main", "stream", "monitor", "monitor2", "monitor3"]);
    assert_eq!(b[4]["peak_l_db"], -6.0);
    assert_eq!(b[3]["peak_l_db"], -40.0);
}

#[test]
fn a_scene_round_trips_the_new_sends() {
    let m = MixerControl::new();
    m.set_strip_param(0, "send_monitor2_db", -3.0).unwrap();
    m.set_strip_param(0, "send_monitor3_db", -7.5).unwrap();
    m.set_bus_param("monitor2", "fader_db", -2.0).unwrap();
    let json = serde_json::to_string(&m.capture_scene("Worship")).unwrap();
    let scene: StoredScene = serde_json::from_str(&json).unwrap();

    let fresh = MixerControl::new();
    fresh.apply_scene(&scene);
    let s = &fresh.params.strips[0];
    assert_eq!(s.send_db[BUS_MONITOR2].load(Relaxed), -3.0);
    assert_eq!(s.send_db[BUS_MONITOR3].load(Relaxed), -7.5);
    assert_eq!(fresh.params.buses[BUS_MONITOR2].fader_db.load(Relaxed), -2.0);
}

#[test]
fn an_old_scene_file_turns_the_new_sends_off() {
    // A scene saved before mon 2 / mon 3: three sends, three buses.
    let m = MixerControl::new();
    let mut value = serde_json::to_value(m.capture_scene("Old")).unwrap();
    for strip in value["mixer"]["strips"].as_array_mut().unwrap() {
        strip["send_db"] = serde_json::json!([0.0, 0.0, -4.0]);
    }
    value["mixer"]["buses"].as_array_mut().unwrap().truncate(3);
    let scene: StoredScene = serde_json::from_value(value).unwrap();

    let live = MixerControl::new();
    live.set_strip_param(0, "send_monitor2_db", 0.0).unwrap();
    live.set_strip_param(1, "send_monitor3_db", -1.0).unwrap();
    live.apply_scene(&scene);
    for strip in live.params.strips.iter().take(2) {
        assert_eq!(strip.send_db[BUS_MONITOR].load(Relaxed), -4.0);
        assert_eq!(strip.send_db[BUS_MONITOR2].load(Relaxed), -90.0);
        assert_eq!(strip.send_db[BUS_MONITOR3].load(Relaxed), -90.0);
    }
}

use super::{
    Mixer, MixerParams, MixerScene, BUS_MAIN, BUS_MONITOR, BUS_STREAM,
};
use std::sync::{atomic::Ordering::Relaxed, Arc};

fn signal_on(mixer: &mut Mixer, bus: usize) -> f32 {
    let source = [0.2; 512];
    for _ in 0..4 {
        mixer.process(&[&source, &source], 512);
    }
    mixer.output(bus).0[511]
}

fn near(value: f32, expected: f32) {
    assert!((value - expected).abs() < 0.001, "expected {expected}, got {value}");
}

#[test]
fn dca_moves_effective_output_while_member_faders_and_prefader_sends_stay_put() {
    let p = Arc::new(MixerParams::new(2));
    p.strips[0].groups.dca.store(0b0011, Relaxed);
    p.strips[0].send_db[BUS_STREAM].store(-90.0, Relaxed);
    p.strips[0].send_db[BUS_MONITOR].store(0.0, Relaxed);
    p.strips[1].send_db[BUS_MAIN].store(-90.0, Relaxed);
    p.groups.faders[0].store(-6.0, Relaxed);
    p.groups.faders[1].store(-6.0, Relaxed);

    let mut mixer = Mixer::new(48_000.0, 512, 2, p.clone());
    near(signal_on(&mut mixer, BUS_MAIN), 0.2 * 10f32.powf(-12.0 / 20.0));
    near(signal_on(&mut mixer, BUS_STREAM), 0.2);
    near(signal_on(&mut mixer, BUS_MONITOR), 0.2);
    assert_eq!(p.strips[0].fader_db.load(Relaxed), 0.0);

    p.strips[0].monitor_post_fader.store(true, Relaxed);
    near(signal_on(&mut mixer, BUS_MONITOR), 0.2 * 10f32.powf(-12.0 / 20.0));
    p.groups.faders[0].store(-90.0, Relaxed);
    p.groups.faders[1].store(10.0, Relaxed);
    near(signal_on(&mut mixer, BUS_MAIN), 0.0);
}

#[test]
fn overlapping_mute_reasons_release_independently_and_pfl_is_available() {
    let p = Arc::new(MixerParams::new(2));
    let member = &p.strips[0];
    member.groups.dca.store(1, Relaxed);
    member.groups.mute.store(2, Relaxed);
    member.send_db[BUS_MONITOR].store(0.0, Relaxed);
    p.strips[1].send_db[BUS_MAIN].store(-90.0, Relaxed);
    let mut mixer = Mixer::new(48_000.0, 512, 2, p.clone());

    p.groups.dca_muted.store(1, Relaxed);
    p.groups.mute_active.store(2, Relaxed);
    near(signal_on(&mut mixer, BUS_MAIN), 0.0);
    p.groups.dca_muted.store(0, Relaxed);
    near(signal_on(&mut mixer, BUS_MONITOR), 0.0);
    member.solo.store(true, Relaxed);
    near(signal_on(&mut mixer, BUS_MONITOR), 0.2);
    member.mute.store(true, Relaxed);
    p.groups.mute_active.store(0, Relaxed);
    near(signal_on(&mut mixer, BUS_MAIN), 0.0);
    member.mute.store(false, Relaxed);
    near(signal_on(&mut mixer, BUS_MAIN), 0.2);
}

#[test]
fn scene_round_trip_and_legacy_defaults_cover_groups_and_membership() {
    let p = MixerParams::new(2);
    p.strips[1].groups.dca.store(128, Relaxed);
    p.strips[1].groups.mute.store(4, Relaxed);
    p.groups.faders[7].store(-15.0, Relaxed);
    p.groups.dca_muted.store(128, Relaxed);
    p.groups.mute_active.store(4, Relaxed);

    let scene = MixerScene::capture(&p);
    let mut json = serde_json::to_value(&scene).unwrap();
    let copy: MixerScene = serde_json::from_value(json.clone()).unwrap();
    let fresh = MixerParams::new(2);
    copy.apply(&fresh);
    assert_eq!(MixerScene::capture(&fresh), scene);

    json.as_object_mut().unwrap().remove("groups");
    for strip in json["strips"].as_array_mut().unwrap() {
        strip.as_object_mut().unwrap().remove("group_membership");
    }
    let old: MixerScene = serde_json::from_value(json).unwrap();
    old.apply(&fresh);
    assert_eq!(fresh.groups.faders[7].load(Relaxed), 0.0);
    assert_eq!(fresh.groups.dca_muted.load(Relaxed), 0);
    assert_eq!(fresh.groups.mute_active.load(Relaxed), 0);
    assert_eq!(fresh.strips[1].groups.dca.load(Relaxed), 0);
    assert_eq!(fresh.strips[1].groups.mute.load(Relaxed), 0);
}

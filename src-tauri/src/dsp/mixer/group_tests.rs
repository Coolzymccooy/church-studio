use super::{
    Mixer, MixerParams, MixerScene, BUS_MAIN, BUS_MONITOR, BUS_MONITOR2, BUS_MONITOR3, BUS_STREAM,
};
use std::sync::{atomic::Ordering::Relaxed, Arc};

fn signal_on(mixer: &mut Mixer, bus: usize) -> f32 {
    mixer.params.groups.publish(&mixer.params.strips);
    let source = [0.2; 512];
    for _ in 0..4 {
        mixer.process(&[&source, &source], 512);
    }
    mixer.output(bus).0[511]
}

fn near(value: f32, expected: f32) {
    assert!(
        (value - expected).abs() < 0.001,
        "expected {expected}, got {value}"
    );
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
    near(
        signal_on(&mut mixer, BUS_MAIN),
        0.2 * 10f32.powf(-12.0 / 20.0),
    );
    near(signal_on(&mut mixer, BUS_STREAM), 0.2);
    near(signal_on(&mut mixer, BUS_MONITOR), 0.2);
    assert_eq!(p.strips[0].fader_db.load(Relaxed), 0.0);

    p.strips[0].monitor_post_fader.store(true, Relaxed);
    near(
        signal_on(&mut mixer, BUS_MONITOR),
        0.2 * 10f32.powf(-12.0 / 20.0),
    );
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

#[test]
fn strip_and_dca_gain_share_one_ceiling_on_main_and_stream() {
    let p = Arc::new(MixerParams::new(1));
    p.strips[0].groups.dca.store(1, Relaxed);
    p.strips[0].fader_db.store(10.0, Relaxed);
    p.groups.faders[0].store(10.0, Relaxed);
    let mut mixer = Mixer::new(48_000.0, 512, 1, p.clone());
    let expected = 0.2 * 10f32.powf(10.0 / 20.0);
    near(signal_on(&mut mixer, BUS_MAIN), expected);
    near(signal_on(&mut mixer, BUS_STREAM), expected);
}

#[test]
fn dca_moves_ramp_and_prefader_monitor_two_and_three_keep_their_level() {
    let p = Arc::new(MixerParams::new(1));
    p.strips[0].groups.dca.store(1, Relaxed);
    for bus in [BUS_MONITOR2, BUS_MONITOR3] {
        p.strips[0].send_db[bus].store(0.0, Relaxed);
    }
    let mut mixer = Mixer::new(48_000.0, 512, 1, p.clone());
    let mut last = signal_on(&mut mixer, BUS_MAIN);
    p.groups.faders[0].store(-18.0, Relaxed);
    p.groups.publish(&p.strips);
    for _ in 0..4 {
        mixer.process(&[&[0.2; 512]], 512);
        for &sample in mixer.output(BUS_MAIN).0 {
            assert!((sample - last).abs() < 0.001, "{last} -> {sample}");
            last = sample;
        }
        for bus in [BUS_MONITOR2, BUS_MONITOR3] {
            near(mixer.output(bus).0[511], 0.2);
        }
    }
    near(last, 0.2 * 10f32.powf(-18.0 / 20.0));
    assert!(mixer.dca_peaks()[0] > -33.0);
}

#[test]
fn recall_publishes_master_and_membership_together() {
    use super::group_publication::GroupSnapshot;
    let p = Arc::new(MixerParams::new(1));
    let mut a = MixerScene::capture(&p);
    a.groups.mute_active = 1;
    a.strips[0].group_membership.mute = 1;
    let mut b = a.clone();
    b.groups.mute_active = 2;
    b.strips[0].group_membership.mute = 2;
    a.apply(&p);

    let writer = p.clone();
    let job = std::thread::spawn(move || {
        for _ in 0..500 {
            b.apply(&writer);
            a.apply(&writer);
        }
    });
    let mut cached = GroupSnapshot::default();
    while !job.is_finished() {
        p.groups.refresh_audio(&mut cached);
        assert!(cached.state.muted(cached.members[0]));
    }
    job.join().unwrap();
    p.groups.refresh_audio(&mut cached);
    assert_eq!(cached.members[0].mute, 1);
}

#[test]
fn dca_cut_ends_automix_speech_and_releases_duck() {
    use super::{group_publication::GroupSnapshot, AutoMix};
    let p = MixerParams::new(2);
    p.strips[0].automix.store(true, Relaxed);
    p.strips[0].groups.dca.store(1, Relaxed);
    p.strips[1].duck.store(true, Relaxed);
    let mut auto = AutoMix::new(48_000.0, 480, 2);
    auto.set_vad_override(0, Some(1.0));
    p.groups.publish(&p.strips);
    let mut cached = GroupSnapshot::default();
    p.groups.refresh_audio(&mut cached);
    for _ in 0..100 {
        auto.process(&p.strips, 480, |_| &[0.0001; 480], &cached);
    }
    for _ in 0..100 {
        auto.process(&p.strips, 480, |_| &[0.25; 480], &cached);
    }
    assert!(auto.readout(0).speech);
    assert!(auto.readout(1).duck_gr_db < -4.0);

    p.groups.faders[0].store(-90.0, Relaxed);
    p.groups.publish(&p.strips);
    p.groups.refresh_audio(&mut cached);
    for _ in 0..300 {
        auto.process(&p.strips, 480, |_| &[0.25; 480], &cached);
    }
    assert!(!auto.readout(0).speech);
    assert!(auto.readout(1).duck_gr_db > -2.0);
}

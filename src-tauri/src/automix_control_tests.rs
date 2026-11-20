//! Control side of the automixer and auto-ducking: keys, exclusivity,
//! scenes (including files saved before automix existed), the music default
//! for computer-audio strips, and the live readouts in `mixer_state`.
use crate::mixer_control::{
    engine_strip_feeds, MixerControl, StoredScene,
};
use std::sync::atomic::Ordering::Relaxed;

#[test]
fn automix_keys_default_off_and_clamp() {
    let m = MixerControl::new();
    let s = &m.snapshot(2, false, Vec::new()).strips[1];
    assert!(!s.automix && !s.duck);
    assert_eq!(s.duck_depth_db, 12.0);

    m.set_strip_param(1, "duck_depth_db", 99.0).unwrap();
    assert_eq!(m.params.strips[1].duck_depth_db.load(Relaxed), 40.0);
    m.set_strip_param(1, "duck_depth_db", -5.0).unwrap();
    assert_eq!(m.params.strips[1].duck_depth_db.load(Relaxed), 0.0);
    m.set_strip_param(1, "duck_depth_db", f32::NAN).unwrap();
    assert_eq!(m.params.strips[1].duck_depth_db.load(Relaxed), 12.0);
    assert!(m.set_strip_bool(99, "automix", true).is_err());
}

#[test]
fn automix_and_duck_are_exclusive_on_a_strip() {
    let m = MixerControl::new();
    m.set_strip_bool(0, "automix", true).unwrap();
    m.set_strip_bool(0, "duck", true).unwrap();
    let s = &m.snapshot(1, false, Vec::new()).strips[0];
    assert!(s.duck && !s.automix);

    m.set_strip_bool(0, "automix", true).unwrap();
    let s = &m.snapshot(1, false, Vec::new()).strips[0];
    assert!(s.automix && !s.duck);

    // Turning one off leaves the other alone.
    m.set_strip_bool(0, "duck", false).unwrap();
    assert!(m.params.strips[0].automix.load(Relaxed));
}

#[test]
fn scenes_keep_automix_and_old_files_load_with_defaults() {
    let m = MixerControl::new();
    m.set_strip_bool(0, "automix", true).unwrap();
    m.set_strip_bool(1, "duck", true).unwrap();
    m.set_strip_param(1, "duck_depth_db", 18.0).unwrap();
    let scene = m.capture_scene("Sermon");

    let fresh = MixerControl::new();
    fresh.apply_scene(&scene);
    let state = fresh.snapshot(2, false, Vec::new());
    assert!(state.strips[0].automix && state.strips[1].duck);
    assert_eq!(state.strips[1].duck_depth_db, 18.0);

    // A file from before automix: the keys are missing.
    let mut value = serde_json::to_value(&scene).unwrap();
    for strip in value["mixer"]["strips"].as_array_mut().unwrap() {
        let obj = strip.as_object_mut().unwrap();
        obj.remove("automix");
        obj.remove("duck");
        obj.remove("duck_depth_db");
    }
    let old: StoredScene = serde_json::from_value(value).unwrap();
    fresh.apply_scene(&old);
    let state = fresh.snapshot(2, false, Vec::new());
    assert!(!state.strips[0].automix && !state.strips[1].duck);
    assert_eq!(state.strips[1].duck_depth_db, 12.0);

    // A tampered file: out-of-range depth is clamped, both flags → duck.
    let mut value = serde_json::to_value(&scene).unwrap();
    value["mixer"]["strips"][0]["duck"] = serde_json::json!(true);
    value["mixer"]["strips"][0]["duck_depth_db"] = serde_json::json!(500.0);
    let tampered: StoredScene = serde_json::from_value(value).unwrap();
    fresh.apply_scene(&tampered);
    let s = &fresh.snapshot(1, false, Vec::new()).strips[0];
    assert!(s.duck && !s.automix);
    assert_eq!(s.duck_depth_db, 40.0);
}

#[test]
fn computer_audio_strips_start_as_ducked_music() {
    let m = MixerControl::new();
    let feeds = engine_strip_feeds(1, Some(1), &[]);
    assert_eq!(m.link().assign_strip_feeds(&feeds), vec![1, 2]);
    let state = m.snapshot(3, true, Vec::new());
    assert!(state.strips[1].duck && state.strips[2].duck);
    assert!(!state.strips[0].duck);

    // The operator's choice survives the next start with the same feeds.
    m.set_strip_bool(1, "duck", false).unwrap();
    assert!(m.link().assign_strip_feeds(&feeds).is_empty());
    assert!(!m.snapshot(3, true, Vec::new()).strips[1].duck);
}

#[test]
fn adding_a_mic_moves_the_music_strips_with_their_automix_settings() {
    let m = MixerControl::new();
    m.link().assign_strip_feeds(&engine_strip_feeds(1, Some(1), &[]));
    m.set_strip_bool(0, "automix", true).unwrap();
    m.set_strip_param(2, "duck_depth_db", 20.0).unwrap();
    // A second mic appears: the computer pair moves from 1–2 to 2–3.
    m.link().assign_strip_feeds(&engine_strip_feeds(2, Some(2), &[]));
    let state = m.snapshot(4, true, Vec::new());
    assert!(state.strips[0].automix, "mic 1 keeps auto-mix");
    assert!(state.strips[2].duck && state.strips[3].duck, "music keeps duck");
    assert_eq!(state.strips[3].duck_depth_db, 20.0, "depth moved with its strip");
}

#[test]
fn live_readouts_show_only_while_running() {
    let m = MixerControl::new();
    m.params.strips[0].live.automix_gr_db.store(9.5, Relaxed);
    m.params.strips[0].live.duck_gr_db.store(f32::NAN, Relaxed);
    let running = &m.snapshot(1, true, Vec::new()).strips[0];
    assert_eq!(running.automix_gr_db, 9.5);
    assert_eq!(running.duck_gr_db, 0.0);
    let stopped = &m.snapshot(1, false, Vec::new()).strips[0];
    assert_eq!(stopped.automix_gr_db, 0.0);
}

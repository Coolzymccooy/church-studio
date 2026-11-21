//! Tests for record preview playback.
use super::*;
use crate::recorder::wav::{encode_le, header_bytes};
use std::io::Cursor;
use std::time::Instant;

const SR: u32 = 48_000;

/// An in-memory float WAV of `frames` frames, every sample = `value`.
fn wav(channels: u16, sample_rate: u32, frames: usize, value: f32) -> WavReader<Cursor<Vec<u8>>> {
    let samples = vec![value; frames * channels as usize];
    let mut data = Vec::new();
    encode_le(&samples, &mut data);
    let mut bytes = header_bytes(channels, sample_rate, data.len() as u32).to_vec();
    bytes.extend_from_slice(&data);
    WavReader::new(Cursor::new(bytes)).unwrap()
}

/// Drain the tap the way the callback does, in 256-frame blocks.
fn drain(tap: &mut PreviewTap, blocks: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut l, mut r) = (Vec::new(), Vec::new());
    let (mut bl, mut br) = (vec![0.0; 256], vec![0.0; 256]);
    for _ in 0..blocks {
        tap.pop_into(&mut bl, &mut br);
        l.extend_from_slice(&bl);
        r.extend_from_slice(&br);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    (l, r)
}

fn wait_until(mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() && start.elapsed().as_secs() < 5 {
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn an_idle_tap_gives_silence() {
    let (mut tap, _player) = preview_ring(SR);
    let (mut l, mut r) = (vec![1.0; 64], vec![1.0; 64]);
    assert_eq!(tap.pop_into(&mut l, &mut r), 0);
    assert!(l.iter().chain(r.iter()).all(|s| *s == 0.0));
}

#[test]
fn a_mono_recording_plays_on_both_sides_then_stops_by_itself() {
    let (mut tap, player) = preview_ring(SR);
    player.play(vec![wav(1, SR, 4_800, 0.25)]).unwrap();
    assert!(player.status().playing);
    assert!((player.status().duration_seconds - 0.1).abs() < 1e-6);
    wait_until(|| player.state.position_frames.load(Ordering::Relaxed) == 4_800);
    let (l, r) = drain(&mut tap, 30);
    assert_eq!(l.iter().filter(|s| **s == 0.25).count(), 4_800);
    assert_eq!(l, r);
    wait_until(|| !player.status().playing);
    assert!(!player.status().playing, "ends once the ring drains");
}

#[test]
fn parts_play_in_order_and_stereo_keeps_its_sides() {
    let (mut tap, player) = preview_ring(SR);
    let mut a = vec![0.0f32; 2 * 256];
    for f in 0..256 {
        a[2 * f] = 0.1;
        a[2 * f + 1] = 0.2;
    }
    let mut data = Vec::new();
    encode_le(&a, &mut data);
    let mut bytes = header_bytes(2, SR, data.len() as u32).to_vec();
    bytes.extend_from_slice(&data);
    let first = WavReader::new(Cursor::new(bytes)).unwrap();
    player.play(vec![first, wav(2, SR, 256, 0.5)]).unwrap();
    wait_until(|| player.state.position_frames.load(Ordering::Relaxed) == 512);
    let (l, r) = drain(&mut tap, 2);
    assert!(l[..256].iter().all(|s| *s == 0.1) && r[..256].iter().all(|s| *s == 0.2));
    assert!(l[256..512].iter().all(|s| *s == 0.5));
}

#[test]
fn stop_silences_at_once_and_a_new_play_can_start() {
    let (mut tap, player) = preview_ring(SR);
    player.play(vec![wav(1, SR, SR as usize * 5, 0.3)]).unwrap();
    wait_until(|| player.state.position_frames.load(Ordering::Relaxed) > 1_000);
    player.stop();
    let (l, _) = drain(&mut tap, 1);
    assert!(l.iter().all(|s| *s == 0.0), "queued audio dropped on stop");
    wait_until(|| !player.status().playing);
    assert!(player.play(vec![wav(1, SR, 256, 0.1)]).is_ok(), "the player is reusable");
}

#[test]
fn refuses_a_second_play_and_a_different_sample_rate() {
    let (_tap, player) = preview_ring(SR);
    let err = player.play(vec![wav(1, 44_100, 256, 0.1)]).unwrap_err();
    assert!(err.contains("44100"), "{err}");
    player.play(vec![wav(1, SR, SR as usize * 5, 0.1)]).unwrap();
    assert_eq!(player.play(vec![wav(1, SR, 256, 0.1)]).unwrap_err(), "already playing");
    player.stop();
}

#[test]
fn to_stereo_doubles_mono_and_drops_extra_channels() {
    let mut out = Vec::new();
    to_stereo(&[1.0, 2.0], 1, 2, &mut out);
    assert_eq!(out, vec![1.0, 1.0, 2.0, 2.0]);
    to_stereo(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 3, 2, &mut out);
    assert_eq!(out, vec![1.0, 2.0, 4.0, 5.0]);
}

#[test]
fn audio_pushed_after_stop_is_never_heard() {
    // The reader thread can finish one last push just after `stop`; the tap
    // must stay silent until the next play rather than play that tail.
    let (mut tap, player) = preview_ring(SR);
    let mut producer = player.producer.lock().unwrap().take().unwrap();
    player.stop();
    let (mut l, mut r) = (vec![1.0; 256], vec![1.0; 256]);
    tap.pop_into(&mut l, &mut r); // the block that sees the stop
    producer.push_slice(&[0.5; 512]); // the reader's late push
    assert_eq!(tap.pop_into(&mut l, &mut r), 0);
    assert!(l.iter().chain(r.iter()).all(|s| *s == 0.0));
}

#[test]
fn the_player_is_ready_again_as_soon_as_it_reports_stopped() {
    for _ in 0..20 {
        let (_tap, player) = preview_ring(SR);
        player.play(vec![wav(1, SR, SR as usize * 5, 0.3)]).unwrap();
        player.stop();
        wait_until(|| !player.status().playing);
        assert!(player.play(vec![wav(1, SR, 256, 0.1)]).is_ok(), "already playing right after stop");
        player.stop();
    }
}

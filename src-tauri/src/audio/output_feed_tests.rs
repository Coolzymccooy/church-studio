use super::output_feed::{map_frame, pair_fits, ClockMode, OutputFeed, OutputStats};
use ringbuf::traits::{Producer, Split};
use ringbuf::HeapRb;
use std::sync::Arc;

#[test]
fn pairs_land_on_their_channels_and_the_rest_is_silent() {
    // Two buses: bus A on 3-4, bus B on 1-2, on a 6-channel device.
    let src = [0.1, 0.2, 0.3, 0.4];
    let mut out = [9.0f32; 6];
    map_frame(&src, &[Some(2), Some(0)], 6, &mut out);
    assert_eq!(out, [0.3, 0.4, 0.1, 0.2, 0.0, 0.0]);
}

#[test]
fn a_mono_device_gets_the_sum_of_left_and_right_halved() {
    let mut out = [9.0f32; 1];
    map_frame(&[0.2, 0.6], &[Some(0)], 1, &mut out);
    assert!((out[0] - 0.4).abs() < 1e-6);
}

#[test]
fn a_member_that_does_not_fit_is_not_played() {
    let mut out = [9.0f32; 2];
    map_frame(&[0.5, 0.5, 0.7, 0.7], &[Some(0), None], 2, &mut out);
    assert_eq!(out, [0.5, 0.5]);
}

#[test]
fn pair_fit_rules() {
    assert!(pair_fits(0, 1));
    assert!(!pair_fits(1, 1));
    assert!(pair_fits(0, 2));
    assert!(!pair_fits(1, 2));
    assert!(pair_fits(1, 4));
    assert!(!pair_fits(2, 4));
    assert!(pair_fits(8, 18));
}

fn push_frames<P: Producer<Item = f32>>(p: &mut P, frames: usize, width: usize, value: f32) {
    for _ in 0..frames {
        for _ in 0..width {
            p.try_push(value).unwrap();
        }
    }
}

#[test]
fn shared_clock_reads_whole_frames_only() {
    let (mut p, c) = HeapRb::<f32>::new(1_000).split();
    let stats = Arc::new(OutputStats::default());
    let mut feed = OutputFeed::new(c, vec![0], ClockMode::Shared, 48_000, 256, stats.clone());
    feed.set_device_channels(2);
    // One and a half frames: only one may be read; the half frame waits.
    p.try_push(0.5).unwrap();
    p.try_push(-0.5).unwrap();
    p.try_push(0.25).unwrap();
    let mut out = vec![9.0f32; 4];
    feed.fill(&mut out, |s| s);
    assert_eq!(out, [0.5, -0.5, 0.0, 0.0]);
    assert_eq!(stats.underruns(), 1);
    p.try_push(-0.25).unwrap();
    feed.fill(&mut out, |s| s);
    assert_eq!(&out[..2], &[0.25, -0.25], "channel order kept after a short read");
}

#[test]
fn a_pair_beyond_the_device_is_reported() {
    let (_p, c) = HeapRb::<f32>::new(100).split();
    let stats = Arc::new(OutputStats::default());
    let mut feed = OutputFeed::new(c, vec![0, 1], ClockMode::Shared, 48_000, 256, stats);
    assert_eq!(feed.set_device_channels(2), vec![1]);
    assert_eq!(feed.set_device_channels(4), Vec::<usize>::new());
}

/// Feed the ring at `ppm` away from the output clock for a simulated minute.
fn simulate(ppm: f64) -> (u64, f64) {
    let rate = 48_000u32;
    let block = 256usize;
    let width = 4usize;
    let (mut p, c) = HeapRb::<f32>::new(rate as usize * width).split();
    let stats = Arc::new(OutputStats::default());
    let mut feed = OutputFeed::new(c, vec![0, 1], ClockMode::Corrected, rate, block, stats.clone());
    feed.set_device_channels(4);
    let per_block = block as f64 * (1.0 + ppm / 1e6);
    let mut owed = 0.0;
    let mut out = vec![0.0f32; block * 4];
    let mut late_underruns = 0;
    for step in 0..(60 * rate as usize / block) {
        owed += per_block;
        let frames = owed.floor() as usize;
        owed -= frames as f64;
        push_frames(&mut p, frames, width, 0.1);
        let before = stats.underruns();
        feed.fill(&mut out, |s| s);
        if step > 200 && stats.underruns() != before {
            late_underruns += 1;
        }
    }
    (late_underruns, stats.drift_ppm() as f64)
}

#[test]
fn corrected_clock_absorbs_drift_both_ways() {
    for ppm in [-300.0, 0.0, 300.0] {
        let (underruns, drift) = simulate(ppm);
        assert_eq!(underruns, 0, "ppm {ppm}");
        assert!((drift - ppm).abs() < 150.0, "ppm {ppm}: reported {drift}");
    }
}

#[test]
fn corrected_clock_plays_the_signal_on_its_pair() {
    let (mut p, c) = HeapRb::<f32>::new(48_000).split();
    let stats = Arc::new(OutputStats::default());
    let mut feed = OutputFeed::new(c, vec![1], ClockMode::Corrected, 48_000, 256, stats);
    feed.set_device_channels(4);
    push_frames(&mut p, 2_000, 2, 0.3);
    let mut out = vec![9.0f32; 64 * 4];
    feed.fill(&mut out, |s| s);
    for frame in out.chunks(4) {
        assert_eq!(frame[0], 0.0);
        assert_eq!(frame[1], 0.0);
        assert!((frame[2] - 0.3).abs() < 1e-6, "{frame:?}");
        assert!((frame[3] - 0.3).abs() < 1e-6, "{frame:?}");
    }
}

#[test]
fn restart_drops_stale_audio() {
    let (mut p, c) = HeapRb::<f32>::new(10_000).split();
    let stats = Arc::new(OutputStats::default());
    let mut feed = OutputFeed::new(c, vec![0], ClockMode::Shared, 48_000, 256, stats);
    feed.set_device_channels(2);
    push_frames(&mut p, 1_000, 2, 0.9);
    feed.restart();
    let mut out = vec![9.0f32; 8];
    feed.fill(&mut out, |s| s);
    assert!(out.iter().all(|&s| s == 0.0), "{out:?}");
}

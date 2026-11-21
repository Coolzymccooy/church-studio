//! USB interface wiring that needs no device: input channel ranges, bus
//! frames for shared devices, strip names, and the no-common-rate error
//! (`docs/specs/2026-11-20-usb-interfaces.md`).
use super::config::{no_common_rate_error, resolve_input_range, RateSpan};
use super::open::missing_name;
use crate::mixer_control::label_hardware_strips;
use crate::mixer_control::{engine_strip_feeds, HARDWARE_STRIP};
use crate::routing::{deinterleave_from, push_bus_frames};
use ringbuf::traits::{Consumer, Split};
use ringbuf::HeapRb;

#[test]
fn strips_read_from_the_first_chosen_channel() {
    // 4-channel device, strips from channel 3 (index 2).
    let data = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
    let mut outs = vec![vec![0.0f32; 4]; 2];
    let frames = deinterleave_from(&data, 4, 2, &mut outs, |s| s);
    assert_eq!(frames, 2);
    assert_eq!(&outs[0][..2], &[3.0, 7.0]);
    assert_eq!(&outs[1][..2], &[4.0, 8.0]);
}

#[test]
fn channels_past_the_device_are_left_untouched() {
    let data = [1.0f32, 2.0, 3.0, 4.0];
    let mut outs = vec![vec![9.0f32; 2]; 2];
    deinterleave_from(&data, 2, 1, &mut outs, |s| s);
    assert_eq!(&outs[0][..2], &[2.0, 4.0]);
    assert_eq!(&outs[1][..2], &[9.0, 9.0]);
}

#[test]
fn shared_device_frames_interleave_each_bus_as_a_pair() {
    let (mut p, mut c) = HeapRb::<f32>::new(64).split();
    let (a_l, a_r) = ([0.1f32, 0.2], [0.3f32, 0.4]);
    let (b_l, b_r) = ([0.5f32, 0.6], [0.7f32, 0.8]);
    let dropped = push_bus_frames(&mut p, &[(&a_l, &a_r, 1.0), (&b_l, &b_r, 0.5)], 2);
    assert_eq!(dropped, 0);
    let got: Vec<f32> = c.pop_iter().collect();
    assert_eq!(got, vec![0.1, 0.3, 0.25, 0.35, 0.2, 0.4, 0.3, 0.4]);
}

#[test]
fn a_full_ring_drops_whole_frames() {
    let (mut p, mut c) = HeapRb::<f32>::new(6).split();
    let l = [1.0f32; 3];
    let r = [2.0f32; 3];
    let dropped = push_bus_frames(&mut p, &[(&l, &r, 1.0), (&l, &r, 1.0)], 3);
    assert_eq!(dropped, 8);
    assert_eq!(c.pop_iter().count(), 4);
}

#[test]
fn input_range_defaults_to_every_channel() {
    assert_eq!(resolve_input_range(None, 18, "X-Air"), Ok((0, 18)));
    assert_eq!(resolve_input_range(Some((2, None)), 18, "X-Air"), Ok((2, 16)));
    assert_eq!(resolve_input_range(Some((0, Some(8))), 18, "X-Air"), Ok((0, 8)));
    // A count past the end is cut to the last channel.
    assert_eq!(resolve_input_range(Some((16, Some(8))), 18, "X-Air"), Ok((16, 2)));
    // A zero count means one channel.
    assert_eq!(resolve_input_range(Some((3, Some(0))), 4, "UMC"), Ok((3, 1)));
}

#[test]
fn an_input_range_past_the_device_is_refused_by_name() {
    let err = resolve_input_range(Some((4, None)), 4, "UMC404HD 192k").unwrap_err();
    assert!(err.contains("UMC404HD 192k") && err.contains("4 channels"), "{err}");
}

#[test]
fn hardware_strips_are_named_after_their_device_channel() {
    let mut feeds = engine_strip_feeds(3, None, &[]);
    label_hardware_strips(&mut feeds, 2);
    let labels: Vec<Option<String>> = feeds.iter().map(|f| f.label.clone()).collect();
    assert_eq!(
        labels,
        vec![Some("Ch 3".into()), Some("Ch 4".into()), Some("Ch 5".into())]
    );
    assert!(feeds.iter().all(|f| f.identity == HARDWARE_STRIP));
}

#[test]
fn the_no_common_rate_error_names_every_device_and_its_rates() {
    let err = no_common_rate_error(&[
        RateSpan::new("UMC404HD 192k", &[(44_100, 192_000)]),
        RateSpan::new("AirPods", &[(24_000, 24_000)]),
    ]);
    assert_eq!(
        err,
        "No common sample rate: UMC404HD 192k (44.1-192 kHz), AirPods (24 kHz)"
    );
}

#[test]
fn several_rate_ranges_are_listed() {
    let err = no_common_rate_error(&[RateSpan::new(
        "Old interface",
        &[(44_100, 44_100), (48_000, 48_000), (44_100, 44_100)],
    )]);
    assert_eq!(err, "No common sample rate: Old interface (44.1, 48 kHz)");
}

#[test]
fn a_missing_legacy_id_waits_for_its_name() {
    assert_eq!(missing_name("3:UMC404HD 192k"), "UMC404HD 192k");
    assert_eq!(missing_name("UMC404HD 192k"), "UMC404HD 192k");
    // A name that only contains a colon is kept whole.
    assert_eq!(missing_name("Speakers: Rear"), "Speakers: Rear");
}

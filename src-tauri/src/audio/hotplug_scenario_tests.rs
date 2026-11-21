//! Mid-service hot-plug with fake device lists: the watcher's decisions
//! (`device_names::match_listed` + `health::Reconnector`) for three streams
//! while devices come and go (`docs/specs/2026-11-20-usb-interfaces.md` §2, §3).
use super::device_names::{match_listed, ListedDevice};
use super::health::{LinkState, Reconnector, Step};

/// One engine output stream as the watcher sees it.
struct Stream {
    name: &'static str,
    bound: Option<&'static str>,
    state: LinkState,
    reconnector: Reconnector,
}

impl Stream {
    fn ok(name: &'static str, uid: &'static str) -> Self {
        Stream { name, bound: Some(uid), state: LinkState::Ok, reconnector: Reconnector::default() }
    }
}

fn list(entries: &[(&str, &str)]) -> Vec<ListedDevice> {
    entries
        .iter()
        .map(|(name, uid)| ListedDevice { name: name.to_string(), uid: Some(uid.to_string()) })
        .collect()
}

/// One watcher tick over every stream: what it decided, with lost and
/// reopened states applied the way the engine applies them.
fn tick(streams: &mut [Stream], listed: &[ListedDevice]) -> Vec<Step> {
    // Every other stream's device is reserved, connected or not (as the
    // watcher does), so a returning unit is only ever taken by its own stream.
    let taken: Vec<String> = streams.iter().filter_map(|s| s.bound.map(str::to_string)).collect();
    streams
        .iter_mut()
        .map(|stream| {
            let others: Vec<String> =
                taken.iter().filter(|uid| Some(uid.as_str()) != stream.bound).cloned().collect();
            let target = match_listed(stream.name, stream.bound, listed, &others);
            let step = stream.reconnector.tick(stream.state, target.is_some());
            match step {
                Step::MarkLost => stream.state = LinkState::Lost,
                Step::Reopen => stream.state = LinkState::Ok,
                Step::Wait => {}
            }
            step
        })
        .collect()
}

#[test]
fn a_behringer_unplugged_and_replugged_mid_service() {
    let mut streams = [
        Stream::ok("UMC404HD 192k", "usb-umc"),
        Stream::ok("Jabra Engage 50", "usb-jabra"),
        Stream::ok("MacBook Pro Speakers", "builtin"),
    ];
    let all = list(&[
        ("UMC404HD 192k", "usb-umc"),
        ("Jabra Engage 50", "usb-jabra"),
        ("MacBook Pro Speakers", "builtin"),
    ]);
    let without_umc = list(&[("Jabra Engage 50", "usb-jabra"), ("MacBook Pro Speakers", "builtin")]);
    let wait3 = vec![Step::Wait; 3];

    // Running: nothing to do.
    assert_eq!(tick(&mut streams, &all), wait3);
    // Another device is plugged in elsewhere: the running streams are untouched.
    let mut more = all.clone();
    more.push(ListedDevice { name: "X-Air".into(), uid: Some("usb-xair".into()) });
    assert_eq!(tick(&mut streams, &more), wait3);

    // The UMC is pulled: lost after two ticks; the other buses never change.
    assert_eq!(tick(&mut streams, &without_umc), wait3);
    assert_eq!(tick(&mut streams, &without_umc), vec![Step::MarkLost, Step::Wait, Step::Wait]);
    assert_eq!(streams[0].state, LinkState::Lost);
    assert_eq!(tick(&mut streams, &without_umc), wait3);

    // Plugged back in: reopened once, after it has been listed for two ticks.
    assert_eq!(tick(&mut streams, &all), wait3);
    assert_eq!(tick(&mut streams, &all), vec![Step::Reopen, Step::Wait, Step::Wait]);
    assert_eq!(streams[0].state, LinkState::Ok);
    assert_eq!(tick(&mut streams, &all), wait3);
}

#[test]
fn the_same_model_on_another_port_is_taken_by_name() {
    // The UID changed (another USB port) but nobody else uses that unit.
    let mut streams = [Stream::ok("UMC404HD 192k", "usb-umc-port1")];
    streams[0].state = LinkState::Lost;
    let moved = list(&[("UMC404HD 192k", "usb-umc-port2")]);
    assert_eq!(tick(&mut streams, &moved), vec![Step::Wait]);
    assert_eq!(tick(&mut streams, &moved), vec![Step::Reopen]);
}

#[test]
fn a_failed_listing_does_not_drop_anything() {
    let mut streams = [Stream::ok("UMC404HD 192k", "usb-umc")];
    let all = list(&[("UMC404HD 192k", "usb-umc")]);
    // The watcher keeps the previous list when a listing fails, so an empty
    // list here stands for a real unplug of one tick only.
    assert_eq!(tick(&mut streams, &[]), vec![Step::Wait]);
    assert_eq!(tick(&mut streams, &all), vec![Step::Wait]);
    assert_eq!(streams[0].state, LinkState::Ok);
}

#[test]
fn two_identical_interfaces_drop_out_and_only_the_second_returns() {
    // Codex review on PR 83: both lost; unit B comes back listed as plain "UMC".
    let mut streams = [Stream::ok("UMC", "usb-a"), Stream::ok("UMC (2)", "usb-b")];
    streams[0].state = LinkState::Lost;
    streams[1].state = LinkState::Lost;
    let only_b = list(&[("UMC", "usb-b")]);
    assert_eq!(tick(&mut streams, &only_b), vec![Step::Wait, Step::Wait]);
    assert_eq!(tick(&mut streams, &only_b), vec![Step::Wait, Step::Reopen]);
    assert_eq!(streams[0].state, LinkState::Lost, "unit A's bus must not take unit B");
    assert_eq!(tick(&mut streams, &only_b), vec![Step::Wait, Step::Wait]);
}

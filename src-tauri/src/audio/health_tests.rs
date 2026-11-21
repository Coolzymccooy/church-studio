use super::health::{state_after_error, LinkState, Reconnector, Step, StreamHealth};
use cpal::ErrorKind;

#[test]
fn disconnects_and_invalidated_streams_mean_lost() {
    assert_eq!(state_after_error(ErrorKind::DeviceNotAvailable), Some(LinkState::Lost));
    assert_eq!(state_after_error(ErrorKind::StreamInvalidated), Some(LinkState::Lost));
    assert_eq!(state_after_error(ErrorKind::Xrun), None);
    assert_eq!(state_after_error(ErrorKind::DeviceBusy), None);
}

#[test]
fn stream_health_round_trips_states_and_words() {
    let health = StreamHealth::new(LinkState::Waiting);
    assert_eq!(health.state(), LinkState::Waiting);
    health.set(LinkState::Ok);
    assert_eq!(health.state(), LinkState::Ok);
    for (state, word) in [
        (LinkState::Ok, "ok"),
        (LinkState::Waiting, "waiting"),
        (LinkState::Lost, "lost"),
        (LinkState::Failed, "failed"),
    ] {
        assert_eq!(state.word(), word);
    }
}

#[test]
fn a_working_device_that_vanishes_is_marked_lost() {
    let mut r = Reconnector::default();
    assert_eq!(r.tick(LinkState::Ok, true), Step::Wait);
    assert_eq!(r.tick(LinkState::Ok, false), Step::Wait);
    assert_eq!(r.tick(LinkState::Ok, false), Step::MarkLost);
}

#[test]
fn one_failed_listing_is_not_an_unplug() {
    let mut r = Reconnector::default();
    assert_eq!(r.tick(LinkState::Ok, false), Step::Wait);
    assert_eq!(r.tick(LinkState::Ok, true), Step::Wait);
    assert_eq!(r.tick(LinkState::Ok, false), Step::Wait);
}

#[test]
fn a_stream_remembers_the_device_it_was_opened_on() {
    let health = StreamHealth::new(LinkState::Ok);
    assert_eq!(health.bound(), None);
    health.bind(Some("AppleUSBAudioEngine:Behringer:UMC404HD:1".into()));
    assert_eq!(health.bound().as_deref(), Some("AppleUSBAudioEngine:Behringer:UMC404HD:1"));
}

#[test]
fn a_lost_device_is_reopened_after_two_listed_ticks() {
    let mut r = Reconnector::default();
    assert_eq!(r.tick(LinkState::Lost, false), Step::Wait);
    assert_eq!(r.tick(LinkState::Lost, true), Step::Wait);
    assert_eq!(r.tick(LinkState::Lost, true), Step::Reopen);
}

#[test]
fn a_waiting_device_joins_the_same_way() {
    let mut r = Reconnector::default();
    assert_eq!(r.tick(LinkState::Waiting, true), Step::Wait);
    assert_eq!(r.tick(LinkState::Waiting, true), Step::Reopen);
}

#[test]
fn failed_reopens_are_retried_twice_then_left_until_replugged() {
    let mut r = Reconnector::default();
    r.tick(LinkState::Lost, true);
    assert_eq!(r.tick(LinkState::Lost, true), Step::Reopen);
    assert_eq!(r.tick(LinkState::Failed, true), Step::Reopen);
    assert_eq!(r.tick(LinkState::Failed, true), Step::Reopen);
    assert_eq!(r.tick(LinkState::Failed, true), Step::Wait);
    assert_eq!(r.tick(LinkState::Failed, true), Step::Wait);
    // Unplugged and plugged back: a fresh set of attempts.
    assert_eq!(r.tick(LinkState::Failed, false), Step::Wait);
    assert_eq!(r.tick(LinkState::Failed, true), Step::Wait);
    assert_eq!(r.tick(LinkState::Failed, true), Step::Reopen);
}

#[test]
fn a_reconnected_device_starts_counting_again() {
    let mut r = Reconnector::default();
    r.tick(LinkState::Lost, true);
    assert_eq!(r.tick(LinkState::Lost, true), Step::Reopen);
    assert_eq!(r.tick(LinkState::Ok, true), Step::Wait);
    assert_eq!(r.tick(LinkState::Ok, false), Step::Wait);
    assert_eq!(r.tick(LinkState::Ok, false), Step::MarkLost);
    assert_eq!(r.tick(LinkState::Lost, true), Step::Wait);
    assert_eq!(r.tick(LinkState::Lost, true), Step::Reopen);
}

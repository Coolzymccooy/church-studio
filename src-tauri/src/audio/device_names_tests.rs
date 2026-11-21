use super::device_names::{listed_name_of, match_listed, transport_word, unique_names, ListedDevice};
use cpal::InterfaceType;

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| name.to_string()).collect()
}

#[test]
fn single_devices_keep_their_plain_names() {
    let listed = names(&["UMC404HD 192k", "MacBook Pro Speakers"]);
    assert_eq!(unique_names(&listed), listed);
}

#[test]
fn identical_interfaces_are_told_apart_by_order() {
    let listed = names(&["UMC404HD 192k", "AirPods", "UMC404HD 192k", "UMC404HD 192k"]);
    assert_eq!(
        unique_names(&listed),
        names(&["UMC404HD 192k", "AirPods", "UMC404HD 192k (2)", "UMC404HD 192k (3)"])
    );
}

#[test]
fn a_suffix_never_collides_with_a_real_name() {
    // A device really called "X (2)" keeps it; the second "X" skips to (3).
    let listed = names(&["X", "X (2)", "X"]);
    assert_eq!(unique_names(&listed), names(&["X", "X (2)", "X (3)"]));
}

#[test]
fn transports_have_short_words() {
    assert_eq!(transport_word(InterfaceType::Usb), "usb");
    assert_eq!(transport_word(InterfaceType::BuiltIn), "builtin");
    assert_eq!(transport_word(InterfaceType::Bluetooth), "bluetooth");
    assert_eq!(transport_word(InterfaceType::Virtual), "virtual");
    assert_eq!(transport_word(InterfaceType::Aggregate), "aggregate");
    assert_eq!(transport_word(InterfaceType::Unknown), "other");
}

fn listed(entries: &[(&str, &str)]) -> Vec<ListedDevice> {
    entries
        .iter()
        .map(|(name, uid)| ListedDevice { name: name.to_string(), uid: Some(uid.to_string()) })
        .collect()
}

#[test]
fn the_device_a_stream_was_opened_on_is_found_by_id_first() {
    // Interface #1 (uid A) re-plugged after #2: names swapped, ids did not.
    let list = listed(&[("UMC", "B"), ("UMC (2)", "A")]);
    let found = match_listed("UMC", Some("A"), &list, &[]).unwrap();
    assert_eq!(found.name, "UMC (2)");
}

#[test]
fn a_name_match_never_takes_an_interface_another_stream_uses() {
    // #1 (uid A) unplugged; #2 (uid B, mon 2) is now listed as plain "UMC".
    let list = listed(&[("UMC", "B")]);
    assert_eq!(match_listed("UMC", Some("A"), &list, &["B".to_string()]), None);
    // Nobody uses B: a replacement unit with the same name is taken.
    assert_eq!(match_listed("UMC", Some("A"), &list, &[]).unwrap().uid.as_deref(), Some("B"));
}

#[test]
fn a_device_never_opened_is_matched_by_name() {
    let list = listed(&[("Pastor IEM", "C")]);
    assert_eq!(match_listed("Pastor IEM", None, &list, &[]).unwrap().uid.as_deref(), Some("C"));
    assert_eq!(match_listed("Other", None, &list, &[]), None);
}

#[test]
fn the_default_device_gets_its_unique_listed_name() {
    // Codex review on PR 83: the default is the second of two identical units.
    let list = listed(&[("X", "unit-1"), ("X (2)", "unit-2")]);
    assert_eq!(listed_name_of(&list, Some("unit-2"), "X"), "X (2)");
    assert_eq!(listed_name_of(&list, Some("unit-1"), "X"), "X");
    // No id: the first device with that name, else the raw name.
    assert_eq!(listed_name_of(&list, None, "X"), "X");
    assert_eq!(listed_name_of(&list, Some("gone"), "Y"), "Y");
}

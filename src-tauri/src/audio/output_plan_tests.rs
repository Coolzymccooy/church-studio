use super::output_plan::{plan_outputs, BusRequest, Member};
use crate::dsp::mixer::{BUS_MAIN, BUS_MONITOR, BUS_MONITOR2, BUS_MONITOR3, BUS_STREAM};

fn req(bus: usize, role: &'static str, name: &str, pair: usize, exclusive: bool) -> BusRequest {
    BusRequest {
        bus,
        role,
        name: name.to_string(),
        present: true,
        pair,
        exclusive,
    }
}

fn monitor(name: &str) -> BusRequest {
    req(BUS_MONITOR, "monitor output", name, 0, false)
}

#[test]
fn each_device_gets_one_group_in_opening_order() {
    let plan = plan_outputs(&[
        monitor("Jabra"),
        req(BUS_STREAM, "broadcast output", "BlackHole 2ch", 0, false),
        req(BUS_MAIN, "main output", "UMC404HD", 0, false),
    ])
    .unwrap();
    let names: Vec<&str> = plan.groups.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, ["Jabra", "BlackHole 2ch", "UMC404HD"]);
    assert!(plan.groups.iter().all(|g| g.members.len() == 1 && g.present));
    assert!(plan.skipped.is_empty());
}

#[test]
fn buses_share_an_interface_on_different_pairs() {
    let plan = plan_outputs(&[
        monitor("Jabra"),
        req(BUS_STREAM, "broadcast output", "UMC404HD", 1, false),
        req(BUS_MAIN, "main output", "UMC404HD", 0, false),
        req(BUS_MONITOR2, "monitor 2 output", "X-Air", 2, true),
        req(BUS_MONITOR3, "monitor 3 output", "X-Air", 3, true),
    ])
    .unwrap();
    assert_eq!(plan.groups.len(), 3);
    assert_eq!(
        plan.groups[1].members,
        vec![Member { bus: BUS_STREAM, pair: 1 }, Member { bus: BUS_MAIN, pair: 0 }]
    );
    assert_eq!(plan.groups[1].width(), 4);
    assert_eq!(plan.groups[2].members.len(), 2);
}

#[test]
fn broadcast_or_main_on_a_taken_pair_is_skipped_as_before() {
    let plan = plan_outputs(&[
        monitor("UMC404HD"),
        req(BUS_STREAM, "broadcast output", "UMC404HD", 0, false),
        req(BUS_MAIN, "main output", "UMC404HD", 0, false),
    ])
    .unwrap();
    assert_eq!(plan.groups.len(), 1);
    assert_eq!(plan.groups[0].members.len(), 1);
    assert_eq!(plan.skipped, vec![BUS_STREAM, BUS_MAIN]);
}

#[test]
fn a_monitor_mix_on_a_taken_pair_is_refused_naming_both_and_the_pair() {
    let err = plan_outputs(&[
        monitor("Jabra"),
        req(BUS_MAIN, "main output", "UMC404HD", 1, false),
        req(BUS_MONITOR2, "monitor 2 output", "UMC404HD", 1, true),
    ])
    .unwrap_err();
    assert!(err.contains("monitor 2 output"), "{err}");
    assert!(err.contains("main output"), "{err}");
    assert!(err.contains("UMC404HD"), "{err}");
    assert!(err.contains("3-4"), "{err}");
}

#[test]
fn mon2_and_mon3_cannot_share_a_device_pair() {
    let err = plan_outputs(&[
        monitor("Jabra"),
        req(BUS_MONITOR2, "monitor 2 output", "Pastor IEM", 0, true),
        req(BUS_MONITOR3, "monitor 3 output", "Pastor IEM", 0, true),
    ])
    .unwrap_err();
    assert!(err.contains("monitor 3 output") && err.contains("monitor 2 output"), "{err}");
}

#[test]
fn a_missing_device_is_planned_but_not_present() {
    let mut missing = req(BUS_MONITOR2, "monitor 2 output", "Pastor IEM", 0, true);
    missing.present = false;
    let plan = plan_outputs(&[monitor("Jabra"), missing]).unwrap();
    assert_eq!(plan.groups.len(), 2);
    assert!(!plan.groups[1].present);
    assert_eq!(plan.present_names(), vec!["Jabra".to_string()]);
}

#[test]
fn a_bus_can_be_found_by_group() {
    let plan = plan_outputs(&[
        monitor("Jabra"),
        req(BUS_MAIN, "main output", "UMC404HD", 0, false),
        req(BUS_STREAM, "broadcast output", "UMC404HD", 1, false),
    ])
    .unwrap();
    assert_eq!(plan.device_of(BUS_STREAM), Some("UMC404HD"));
    assert_eq!(plan.device_of(BUS_MONITOR3), None);
}

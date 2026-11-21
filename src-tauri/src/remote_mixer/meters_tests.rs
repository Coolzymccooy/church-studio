use super::meters::{scope, StripTap};
use super::Role;
use crate::mixer_control::BUS_IDS;
use crate::mixer_meters::{BusMeterPayload, StripMeterPayload};

fn strip(pre_db: f32, post_db: f32) -> StripMeterPayload {
    StripMeterPayload {
        pre_db,
        post_db,
        gate_open: true,
        gr_db: 6.0,
        automix_gr_db: 3.0,
        duck_gr_db: 2.0,
        speech: true,
    }
}

fn buses() -> Vec<BusMeterPayload> {
    BUS_IDS
        .iter()
        .enumerate()
        .map(|(i, id)| BusMeterPayload {
            id,
            peak_l_db: -10.0 - i as f32,
            peak_r_db: -20.0 - i as f32,
            rms_l_db: -30.0 - i as f32,
            rms_r_db: -40.0 - i as f32,
            leveler_db: 1.5,
        })
        .collect()
}

const OPEN: StripTap = StripTap {
    muted: false,
    post_fader: false,
};

#[test]
fn a_monitor_role_sees_its_send_taps_and_only_its_own_mix() {
    let strips = [strip(-12.34, -40.0), strip(-6.0, -9.0)];
    let view = scope(Role::Monitor2, &strips, &[OPEN, OPEN], &buses());
    assert_eq!(view.strips, vec![-12.3, -6.0], "pre-fader send tap, 0.1 dB");
    assert_eq!(
        view.mix,
        Some([-13.0, -23.0, -33.0, -43.0]),
        "mon 2 bus only"
    );
    assert!(view.buses.is_empty(), "no other bus reaches a monitor role");

    let json = serde_json::to_string(&view).unwrap();
    for hidden in [
        "main", "stream", "monitor", "gr", "automix", "duck", "speech", "gate", "leveler",
    ] {
        assert!(!json.contains(hidden), "{hidden} leaked into {json}");
    }
}

#[test]
fn each_monitor_role_reads_its_own_bus() {
    let strips = [strip(-12.0, -12.0)];
    let bus = buses();
    assert_eq!(
        scope(Role::Monitor1, &strips, &[OPEN], &bus).mix.unwrap()[0],
        -12.0
    );
    assert_eq!(
        scope(Role::Monitor3, &strips, &[OPEN], &bus).mix.unwrap()[0],
        -14.0
    );
}

#[test]
fn a_muted_strip_reads_silent_and_a_post_fader_mon_one_send_follows_the_fader() {
    let strips = [strip(-6.0, -18.0), strip(-6.0, -18.0)];
    let taps = [
        StripTap {
            muted: true,
            post_fader: false,
        },
        StripTap {
            muted: false,
            post_fader: true,
        },
    ];
    let view = scope(Role::Monitor1, &strips, &taps, &buses());
    assert_eq!(view.strips, vec![-96.0, -18.0]);
}

#[test]
fn the_desk_sees_post_fader_strips_and_every_bus_but_no_personal_mix() {
    let strips = [strip(-6.0, -18.0)];
    let view = scope(Role::Desk, &strips, &[OPEN], &buses());
    assert_eq!(view.strips, vec![-18.0]);
    assert_eq!(view.mix, None);
    assert_eq!(view.buses.len(), BUS_IDS.len());
    assert_eq!(view.buses["monitor3"], [-14.0, -24.0]);
}

#[test]
fn the_strip_count_follows_the_view_and_bad_readings_hit_the_floor() {
    let strips = [
        strip(f32::NAN, -18.0),
        strip(-200.0, -18.0),
        strip(-1.0, -1.0),
    ];
    let view = scope(Role::Monitor2, &strips, &[OPEN, OPEN], &buses());
    assert_eq!(view.strips, vec![-96.0, -96.0]);
    let missing = scope(Role::Monitor2, &strips[..1], &[OPEN, OPEN, OPEN], &buses());
    assert_eq!(missing.strips, vec![-96.0, -96.0, -96.0]);
}

#[test]
fn a_full_monitor_payload_stays_small() {
    let strips: Vec<_> = (0..32).map(|i| strip(-12.3 - i as f32, -1.0)).collect();
    let taps = vec![OPEN; 32];
    let json = serde_json::to_string(&scope(Role::Monitor1, &strips, &taps, &buses())).unwrap();
    assert!(json.len() < 300, "{} bytes: {json}", json.len());
}

//! Live meters for paired remotes. Read-only, built from the desktop's
//! `mixer-meters` readings and cut down to what the paired role may see:
//!
//! - a monitor role gets one level per strip at that mix's send (pre-fader,
//!   or post-fader where mon 1 follows the fader; silent when the strip is
//!   muted, because mute removes a strip from every mix), plus its own mix
//!   bus. While anything is soloed, mon 1 is PFL and reads only the soloed
//!   strips, pre-fader. No other bus, and no dynamics, automix or gate state.
//! - the desk gets each strip after its fader and every bus peak.
//!
//! Values are dBFS rounded to 0.1 dB, so a monitor payload stays a few
//! hundred bytes at the stream's ~16 Hz cadence.
use super::Role;
use crate::mixer_meters::{clamp_db, BusMeterPayload, StripMeterPayload, METER_FLOOR_DB};
use serde::Serialize;
use std::collections::BTreeMap;

/// How one strip reaches the paired monitor mix.
#[derive(Clone, Copy, Debug, Default)]
pub struct StripTap {
    /// Muted on the desk (strip or DCA): the strip is out of every mix.
    pub muted: bool,
    /// The send follows the fader (mon 1 with `monitor_post_fader`).
    pub post_fader: bool,
    /// Soloed on the desk. While anything is soloed, mon 1 is PFL: only the
    /// soloed strips, pre-fader, whether or not they are muted.
    pub soloed: bool,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct MeterView {
    /// One level per strip, in the view's strip order.
    pub strips: Vec<f32>,
    /// Monitor roles: the mix bus as [peak L, peak R, rms L, rms R].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mix: Option<[f32; 4]>,
    /// Desk: every bus as [peak L, peak R], by bus id.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub buses: BTreeMap<&'static str, [f32; 2]>,
}

fn tenth(db: f32) -> f32 {
    (clamp_db(db) * 10.0).round() / 10.0
}

/// Cut the desktop meters down to `role`. `taps` has one entry per strip in
/// the role's view; strips without a reading read as silent.
pub fn scope(
    role: Role,
    strips: &[StripMeterPayload],
    taps: &[StripTap],
    buses: &[BusMeterPayload],
) -> MeterView {
    let pfl = role == Role::Monitor1 && taps.iter().any(|tap| tap.soloed);
    let level = |index: usize, tap: &StripTap| {
        let Some(meter) = strips.get(index) else {
            return METER_FLOOR_DB;
        };
        match role {
            Role::Desk => tenth(meter.post_db),
            _ if pfl && tap.soloed => tenth(meter.pre_db),
            _ if pfl => METER_FLOOR_DB,
            _ if tap.muted => METER_FLOOR_DB,
            _ if tap.post_fader => tenth(meter.post_db),
            _ => tenth(meter.pre_db),
        }
    };
    let strips = taps
        .iter()
        .enumerate()
        .map(|(i, tap)| level(i, tap))
        .collect();
    let Some(bus_id) = role.monitor_bus() else {
        return MeterView {
            strips,
            mix: None,
            buses: buses
                .iter()
                .map(|bus| (bus.id, [tenth(bus.peak_l_db), tenth(bus.peak_r_db)]))
                .collect(),
        };
    };
    let mix = buses.iter().find(|bus| bus.id == bus_id).map(|bus| {
        [
            tenth(bus.peak_l_db),
            tenth(bus.peak_r_db),
            tenth(bus.rms_l_db),
            tenth(bus.rms_r_db),
        ]
    });
    MeterView {
        strips,
        mix,
        buses: BTreeMap::new(),
    }
}

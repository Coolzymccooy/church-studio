//! Lock-free hand-off of mixer meters from the audio callback to the meters
//! thread, which emits the `mixer-meters` event (~20 Hz) with the JSON shape
//! from the live mixer contract.
//!
//! The callback only does atomic loads/stores into slots preallocated at
//! engine start (no allocation, no locks). Peaks and compressor gain
//! reduction are held at their maximum until the meters thread reads them,
//! so short peaks between two reads are not lost. A read racing with a write
//! can lose at most one block's peak, which is harmless for a meter.
use crate::dsp::mixer::{BusMeters, StripMeters, NUM_BUSES};
use crate::mixer_control::BUS_IDS;
use atomic_float::AtomicF32;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

/// dB values in the event are clamped to at least this.
pub const METER_FLOOR_DB: f32 = -96.0;
/// Value a held peak is reset to after each read.
const RESET_DB: f32 = -120.0;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StripMeterPayload {
    pub pre_db: f32,
    pub post_db: f32,
    pub gate_open: bool,
    /// Compressor gain reduction as a positive amount in dB.
    pub gr_db: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BusMeterPayload {
    pub id: &'static str,
    pub peak_l_db: f32,
    pub peak_r_db: f32,
    pub rms_l_db: f32,
    pub rms_r_db: f32,
}

/// `mixer-meters` event payload.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MixerMetersPayload {
    pub strips: Vec<StripMeterPayload>,
    pub buses: Vec<BusMeterPayload>,
}

struct StripSlot {
    pre_db: AtomicF32,
    post_db: AtomicF32,
    gate_open: AtomicBool,
    gr_db: AtomicF32,
}

struct BusSlot {
    peak_l_db: AtomicF32,
    peak_r_db: AtomicF32,
    rms_l_db: AtomicF32,
    rms_r_db: AtomicF32,
}

impl BusSlot {
    fn new() -> Self {
        BusSlot {
            peak_l_db: AtomicF32::new(RESET_DB),
            peak_r_db: AtomicF32::new(RESET_DB),
            rms_l_db: AtomicF32::new(RESET_DB),
            rms_r_db: AtomicF32::new(RESET_DB),
        }
    }
}

pub struct MixerMeterSlots {
    strips: Vec<StripSlot>,
    buses: [BusSlot; NUM_BUSES],
}

/// Clamp a dB reading for the event (NaN → floor).
pub fn clamp_db(db: f32) -> f32 {
    if db.is_nan() {
        METER_FLOOR_DB
    } else {
        db.max(METER_FLOOR_DB)
    }
}

#[inline]
fn hold_max(slot: &AtomicF32, value: f32) {
    if value > slot.load(Relaxed) {
        slot.store(value, Relaxed);
    }
}

/// Read a held value and reset it for the next period.
#[inline]
fn take(slot: &AtomicF32, reset: f32) -> f32 {
    let value = slot.load(Relaxed);
    slot.store(reset, Relaxed);
    value
}

impl MixerMeterSlots {
    /// Slots for `num_strips` strips (sized once, at engine start).
    pub fn new(num_strips: usize) -> Self {
        MixerMeterSlots {
            strips: (0..num_strips)
                .map(|_| StripSlot {
                    pre_db: AtomicF32::new(RESET_DB),
                    post_db: AtomicF32::new(RESET_DB),
                    gate_open: AtomicBool::new(false),
                    gr_db: AtomicF32::new(0.0),
                })
                .collect(),
            buses: [BusSlot::new(), BusSlot::new(), BusSlot::new()],
        }
    }

    /// Audio thread: publish the meters of the block just processed.
    pub fn publish(&self, strips: &[StripMeters], buses: &[BusMeters]) {
        for (slot, m) in self.strips.iter().zip(strips.iter()) {
            hold_max(&slot.pre_db, m.pre_fader_peak_db);
            hold_max(&slot.post_db, m.post_fader_peak_db);
            slot.gate_open.store(m.gate_open, Relaxed);
            hold_max(&slot.gr_db, -m.gain_reduction_db);
        }
        for (slot, m) in self.buses.iter().zip(buses.iter()) {
            hold_max(&slot.peak_l_db, m.peak_l_db);
            hold_max(&slot.peak_r_db, m.peak_r_db);
            slot.rms_l_db.store(m.rms_l_db, Relaxed);
            slot.rms_r_db.store(m.rms_r_db, Relaxed);
        }
    }

    /// Meters thread: build the event payload and reset the held peaks.
    pub fn take_payload(&self) -> MixerMetersPayload {
        let strips = self
            .strips
            .iter()
            .map(|slot| StripMeterPayload {
                pre_db: clamp_db(take(&slot.pre_db, RESET_DB)),
                post_db: clamp_db(take(&slot.post_db, RESET_DB)),
                gate_open: slot.gate_open.load(Relaxed),
                gr_db: {
                    let gr = take(&slot.gr_db, 0.0);
                    if gr.is_nan() {
                        0.0
                    } else {
                        gr.max(0.0)
                    }
                },
            })
            .collect();
        let buses = BUS_IDS
            .iter()
            .zip(self.buses.iter())
            .map(|(id, slot)| BusMeterPayload {
                id: *id,
                peak_l_db: clamp_db(take(&slot.peak_l_db, RESET_DB)),
                peak_r_db: clamp_db(take(&slot.peak_r_db, RESET_DB)),
                rms_l_db: clamp_db(slot.rms_l_db.load(Relaxed)),
                rms_r_db: clamp_db(slot.rms_r_db.load(Relaxed)),
            })
            .collect();
        MixerMetersPayload { strips, buses }
    }
}

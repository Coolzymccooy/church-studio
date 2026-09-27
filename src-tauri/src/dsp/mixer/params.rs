//! Lock-free mixer controls shared between the UI/control thread and the
//! audio thread. Every field is an atomic, same style as `DspParams`: the UI
//! stores, the audio thread loads once per block.
use atomic_float::AtomicF32;
use std::sync::atomic::{AtomicBool, Ordering};

/// Number of stereo buses.
pub const NUM_BUSES: usize = 3;
/// Room / PA.
pub const BUS_MAIN: usize = 0;
/// Livestream / record.
pub const BUS_STREAM: usize = 1;
/// Stage / headphones (also carries solo PFL).
pub const BUS_MONITOR: usize = 2;

/// Fader and send levels at or below this are treated as −inf (silence).
pub const MIN_DB: f32 = -90.0;

pub const TRIM_MIN_DB: f32 = -20.0;
pub const TRIM_MAX_DB: f32 = 40.0;
pub const FADER_MAX_DB: f32 = 10.0;
pub const EQ_MAX_DB: f32 = 15.0;

/// Pan-law normalisation. The pan is constant power (cos/sin), scaled by √2
/// so a centred strip reaches each side of a bus at unity (0 dB) and a hard
/// pan is +3 dB on one side. This keeps a centred mono source at the same
/// level it had before the mixer existed (the old engine pushed the mono
/// voice unchanged to every output channel), both on stereo outputs (L = R =
/// input) and on 1-channel outputs ((L + R) / 2 = input).
pub const PAN_LAW_NORM: f32 = std::f32::consts::SQRT_2;

/// dB → linear gain for faders and sends: ≤ `MIN_DB` (or NaN) is silence.
pub fn fader_db_to_lin(db: f32) -> f32 {
    if db.is_nan() || db <= MIN_DB {
        0.0
    } else {
        10f32.powf(db.min(FADER_MAX_DB) / 20.0)
    }
}

/// Clamp helper that also maps NaN to `fallback`.
pub fn clamp_or(v: f32, lo: f32, hi: f32, fallback: f32) -> f32 {
    if v.is_nan() {
        fallback
    } else {
        v.clamp(lo, hi)
    }
}

/// Controls for one mono input strip.
pub struct StripParams {
    pub trim_db: AtomicF32,
    pub polarity_invert: AtomicBool,
    pub hpf_enabled: AtomicBool,
    pub hpf_freq_hz: AtomicF32,
    pub gate_enabled: AtomicBool,
    pub gate_threshold_db: AtomicF32,
    /// Low shelf 100 Hz, peaking 1 kHz (Q 0.9), high shelf 8 kHz; ±15 dB.
    pub eq_low_db: AtomicF32,
    pub eq_mid_db: AtomicF32,
    pub eq_high_db: AtomicF32,
    pub comp_enabled: AtomicBool,
    pub comp_threshold_db: AtomicF32,
    pub comp_ratio: AtomicF32,
    /// −1 (hard left) .. +1 (hard right), constant-power law normalised to
    /// 0 dB at centre (see `PAN_LAW_NORM`).
    pub pan: AtomicF32,
    pub mute: AtomicBool,
    /// PFL solo to the Monitor bus only.
    pub solo: AtomicBool,
    pub fader_db: AtomicF32,
    /// Send level per bus (index with `BUS_*`).
    pub send_db: [AtomicF32; NUM_BUSES],
    /// Monitor send taken after the fader instead of before it.
    pub monitor_post_fader: AtomicBool,
}

impl StripParams {
    pub fn new() -> Self {
        StripParams {
            trim_db: AtomicF32::new(0.0),
            polarity_invert: AtomicBool::new(false),
            hpf_enabled: AtomicBool::new(false),
            hpf_freq_hz: AtomicF32::new(80.0),
            gate_enabled: AtomicBool::new(false),
            gate_threshold_db: AtomicF32::new(-50.0),
            eq_low_db: AtomicF32::new(0.0),
            eq_mid_db: AtomicF32::new(0.0),
            eq_high_db: AtomicF32::new(0.0),
            comp_enabled: AtomicBool::new(false),
            comp_threshold_db: AtomicF32::new(-18.0),
            comp_ratio: AtomicF32::new(3.0),
            pan: AtomicF32::new(0.0),
            mute: AtomicBool::new(false),
            solo: AtomicBool::new(false),
            fader_db: AtomicF32::new(0.0),
            send_db: [
                AtomicF32::new(0.0),  // Main
                AtomicF32::new(0.0),  // Stream
                AtomicF32::new(MIN_DB), // Monitor: off until a monitor mix is built
            ],
            monitor_post_fader: AtomicBool::new(false),
        }
    }
}

impl Default for StripParams {
    fn default() -> Self {
        Self::new()
    }
}

/// Controls for one stereo bus.
pub struct BusParams {
    pub fader_db: AtomicF32,
    pub mute: AtomicBool,
    pub limiter_enabled: AtomicBool,
    pub limiter_ceiling_db: AtomicF32,
}

impl BusParams {
    pub fn new() -> Self {
        BusParams {
            fader_db: AtomicF32::new(0.0),
            mute: AtomicBool::new(false),
            limiter_enabled: AtomicBool::new(true),
            limiter_ceiling_db: AtomicF32::new(-1.0),
        }
    }
}

impl Default for BusParams {
    fn default() -> Self {
        Self::new()
    }
}

/// All mixer controls. Sized once at construction; never resized.
pub struct MixerParams {
    pub strips: Vec<StripParams>,
    pub buses: [BusParams; NUM_BUSES],
}

impl MixerParams {
    pub fn new(num_strips: usize) -> Self {
        MixerParams {
            strips: (0..num_strips).map(|_| StripParams::new()).collect(),
            buses: [BusParams::new(), BusParams::new(), BusParams::new()],
        }
    }

    /// True if any of the first `active` strips is soloed. The bank holds 32
    /// strips but only the device's channels run, and a solo left on a strip
    /// that no longer has an input must not silence the Monitor bus.
    pub fn any_solo(&self, active: usize) -> bool {
        self.strips.iter().take(active).any(|s| s.solo.load(Ordering::Relaxed))
    }
}

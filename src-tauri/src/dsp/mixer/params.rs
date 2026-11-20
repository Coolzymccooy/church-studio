//! Lock-free mixer controls shared between the UI/control thread and the
//! audio thread. Every field is an atomic, same style as `DspParams`: the UI
//! stores, the audio thread loads once per block.
use super::fx_params::{FxParams, FX_COUNT};
use super::leveler::LEVELER_TARGET_DEFAULT;
use atomic_float::AtomicF32;
use std::sync::atomic::{AtomicBool, Ordering};

/// Number of stereo buses (`docs/specs/2026-11-20-monitor-mixes.md`).
pub const NUM_BUSES: usize = 5;
/// Contract name for `NUM_BUSES`.
pub const BUS_COUNT: usize = NUM_BUSES;
/// Room / PA.
pub const BUS_MAIN: usize = 0;
/// Livestream / record.
pub const BUS_STREAM: usize = 1;
/// Mon 1: the sound engineer's stage / headphones mix (also carries solo
/// PFL, the monitor key and `monitor_post_fader`).
pub const BUS_MONITOR: usize = 2;
/// Mon 2 and mon 3: extra monitor mixes (pastor, worship leader). Their
/// sends are always pre-fader; solo and the monitor key never reach them.
pub const BUS_MONITOR2: usize = 3;
pub const BUS_MONITOR3: usize = 4;

/// Fader and send levels at or below this are treated as −inf (silence).
pub const MIN_DB: f32 = -90.0;

pub const TRIM_MIN_DB: f32 = -20.0;
pub const TRIM_MAX_DB: f32 = 40.0;
pub const FADER_MAX_DB: f32 = 10.0;
pub const EQ_MAX_DB: f32 = 15.0;
/// Auto-duck depth range and default (dB of dip while a voice speaks).
pub const DUCK_DEPTH_MAX_DB: f32 = 40.0;
pub const DUCK_DEPTH_DEFAULT_DB: f32 = 12.0;

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
    /// PFL solo to the Monitor (mon 1) bus only.
    pub solo: AtomicBool,
    pub fader_db: AtomicF32,
    /// Send level per bus (index with `BUS_*`).
    pub send_db: [AtomicF32; NUM_BUSES],
    pub fx_send_db: [AtomicF32; FX_COUNT],
    /// Mon 1 send taken after the fader instead of before it (mon 2 and
    /// mon 3 are always pre-fader).
    pub monitor_post_fader: AtomicBool,
    /// Member of the automix (gain-sharing) group.
    pub automix: AtomicBool,
    /// Music strip: dips by `duck_depth_db` while an automix strip speaks.
    pub duck: AtomicBool,
    pub duck_depth_db: AtomicF32,
    /// Live readouts written by the audio thread once per block (positive dB
    /// of attenuation). Not settings; never saved in scenes.
    pub live: StripLive,
}

/// Live automix/duck attenuation of one strip, for `mixer_state`.
pub struct StripLive {
    pub automix_gr_db: AtomicF32,
    pub duck_gr_db: AtomicF32,
}

impl StripLive {
    pub fn new() -> Self {
        StripLive {
            automix_gr_db: AtomicF32::new(0.0),
            duck_gr_db: AtomicF32::new(0.0),
        }
    }

    pub fn clear(&self) {
        self.automix_gr_db.store(0.0, Ordering::Relaxed);
        self.duck_gr_db.store(0.0, Ordering::Relaxed);
    }
}

impl Default for StripLive {
    fn default() -> Self {
        Self::new()
    }
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
                AtomicF32::new(MIN_DB), // Mon 2: off, a new mix starts silent
                AtomicF32::new(MIN_DB), // Mon 3: off
            ],
            fx_send_db: std::array::from_fn(|_| AtomicF32::new(MIN_DB)),
            monitor_post_fader: AtomicBool::new(false),
            automix: AtomicBool::new(false),
            duck: AtomicBool::new(false),
            duck_depth_db: AtomicF32::new(DUCK_DEPTH_DEFAULT_DB),
            live: StripLive::new(),
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
    /// Loudness leveler (`leveler.rs`): steers the bus toward
    /// `leveler_target_lufs`. A delivery setting, not part of scenes.
    pub leveler_enabled: AtomicBool,
    pub leveler_target_lufs: AtomicF32,
}

impl BusParams {
    pub fn new() -> Self {
        BusParams {
            fader_db: AtomicF32::new(0.0),
            mute: AtomicBool::new(false),
            limiter_enabled: AtomicBool::new(true),
            limiter_ceiling_db: AtomicF32::new(-1.0),
            leveler_enabled: AtomicBool::new(false),
            leveler_target_lufs: AtomicF32::new(LEVELER_TARGET_DEFAULT),
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
    pub fx: FxParams,
}

impl MixerParams {
    pub fn new(num_strips: usize) -> Self {
        MixerParams {
            strips: (0..num_strips).map(|_| StripParams::new()).collect(),
            buses: std::array::from_fn(|_| BusParams::new()),
            fx: FxParams::new(),
        }
    }

    /// True if any of the first `active` strips is soloed. The bank holds 32
    /// strips but only the device's channels run, and a solo left on a strip
    /// that no longer has an input must not silence the Monitor bus.
    pub fn any_solo(&self, active: usize) -> bool {
        self.strips.iter().take(active).any(|s| s.solo.load(Ordering::Relaxed))
    }
}

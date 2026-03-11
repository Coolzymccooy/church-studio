pub mod biquad;
pub mod chain;
pub mod compressor;
pub mod desser;
pub mod dereverb;
pub mod gate;
pub mod lufs;
pub mod noise;

pub use chain::DspChain;

use atomic_float::AtomicF32;
use std::sync::atomic::{AtomicBool, Ordering};

/// Shared DSP parameters — written from Tauri commands, read from audio callback.
/// All fields are atomics for lock-free cross-thread access.
#[derive(Default)]
pub struct DspParams {
    // Input
    pub gain_db:            AtomicF32,
    // Gate
    pub gate_enabled:       AtomicBool,
    pub gate_threshold_db:  AtomicF32,
    // Noise reduction
    pub noise_enabled:      AtomicBool,
    pub noise_alpha:        AtomicF32,
    // EQ (future: make these configurable)
    // Compressor
    pub comp_enabled:       AtomicBool,
    pub comp_threshold_db:  AtomicF32,
    pub comp_ratio:         AtomicF32,
    // De-esser
    pub deess_enabled:      AtomicBool,
    pub deess_threshold_db: AtomicF32,
    // Dereverb
    pub dereverb_enabled:   AtomicBool,
    pub dereverb_strength:  AtomicF32,
    // Global
    pub bypass:             AtomicBool,
    pub monitor_gain_db:    AtomicF32,
}

impl DspParams {
    pub fn defaults() -> Self {
        let p = DspParams::default();
        p.gain_db.store(0.0, Ordering::Relaxed);
        p.gate_enabled.store(true, Ordering::Relaxed);
        p.gate_threshold_db.store(-45.0, Ordering::Relaxed);
        p.noise_enabled.store(false, Ordering::Relaxed);
        p.noise_alpha.store(1.5, Ordering::Relaxed);
        p.comp_enabled.store(true, Ordering::Relaxed);
        p.comp_threshold_db.store(-18.0, Ordering::Relaxed);
        p.comp_ratio.store(3.0, Ordering::Relaxed);
        p.deess_enabled.store(true, Ordering::Relaxed);
        p.deess_threshold_db.store(-24.0, Ordering::Relaxed);
        p.dereverb_enabled.store(false, Ordering::Relaxed);
        p.dereverb_strength.store(0.6, Ordering::Relaxed);
        p.bypass.store(false, Ordering::Relaxed);
        p.monitor_gain_db.store(0.0, Ordering::Relaxed);
        p
    }
}

/// Meter readings sent from audio thread to UI every ~50ms.
#[derive(serde::Serialize, Clone)]
pub struct MetersPayload {
    pub input_db:      f32,
    pub output_db:     f32,
    pub gate_gain:     f32,
    pub lufs_m:        f32,
    pub lufs_st:       f32,
    pub lufs_i:        f32,
    pub deess_gr_db:   f32,
    pub auto_gain_db:  f32,
    /// FFT spectrum bins (0..127) for visualizer
    pub spectrum:      Vec<f32>,
}

/// Device info returned to React
#[derive(serde::Serialize, Clone)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

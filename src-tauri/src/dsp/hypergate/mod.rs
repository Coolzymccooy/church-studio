//! Hyper gate: speech-aware gating for the voice chain, ported from the
//! browser worklet (`public/worklets/hypergate-worklet.js`).
//!
//! - `analysis`: per-frame spectral features (level, voice band, flatness,
//!   zero crossings, formants, fingerprint bands).
//! - `detector`: balanced / speech-only decision and the gain target.
//! - `fingerprint`: "learn mic" fingerprint, learning and matching.
//! - `params`: atomics shared with the Tauri commands.
//!
//! `gate::Gate` runs the detector in place of its level detector when it is
//! in `GateMode::Balanced` or `GateMode::Speech`; the mixer strips keep the
//! plain level gate.
pub mod analysis;
pub mod detector;
pub mod fingerprint;
pub mod params;

#[cfg(test)]
pub(crate) mod signals;
#[cfg(test)]
mod demo;
#[cfg(test)]
mod tests;

pub use detector::{HyperDetector, HyperMode};
pub use fingerprint::{Fingerprint, LearnOutcome};
pub use params::{HyperGateParams, LearnWait};

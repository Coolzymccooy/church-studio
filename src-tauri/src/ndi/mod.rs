//! NDI® audio output (design decisions 1–4 in
//! `docs/specs/2026-09-28-integrations-design.md`).
//!
//! - `runtime`: loads the installed NDI Runtime at run time.
//! - `audio_frame`: interleaved stereo → 480-sample planar float frames.
pub mod audio_frame;
pub mod runtime;

//! OBS Studio control over obs-websocket v5 (design decision 6 in
//! `docs/specs/2026-09-28-integrations-design.md`).
mod config;
mod manager;
mod status;

pub use manager::ObsManager;

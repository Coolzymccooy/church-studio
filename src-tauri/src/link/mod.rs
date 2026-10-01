//! Tiwaton Link: the Studio listens for Lumina Presenter's `lumina-aether`
//! webhook and loads the matching audio scene
//! (`docs/specs/2026-09-28-integrations-design.md`, decisions 7 and 8).
//!
//! - `protocol`: pure request validation and sequence ordering.
//! - `rules`: pure rule matching and the debounce state machine.
//! - `config` / `activity`: settings file and the activity ring.
//! - `server`: the loopback HTTP server (depends only on `LinkBackend`).
//! - `runtime` / `handle` / `commands`: the Tauri glue.
pub mod activity;
pub mod commands;
pub mod config;
pub mod handle;
pub mod protocol;
pub mod rules;
pub mod runtime;
pub mod server;

#[cfg(test)]
mod protocol_tests;
#[cfg(test)]
mod rules_tests;
#[cfg(test)]
mod server_tests;

use std::sync::{Mutex, MutexGuard};

/// Lock a mutex, recovering the data if a panicking thread poisoned it.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

//! Tauri commands for Tiwaton Link (the Lumina Link panel).
//!
//! | Command | Args | Returns |
//! |---|---|---|
//! | `link_get_config` | none | `LinkSnapshot` (port, url, token, enabled, rules, paused, lastEventAt, serverError) |
//! | `link_set_enabled` | `enabled: bool` | `LinkSnapshot` |
//! | `link_set_rules` | `rules: Rule[]` | the saved (cleaned) rules |
//! | `link_regenerate_token` | none | `LinkSnapshot` |
//! | `link_activity` | none | `ActivitySnapshot` (entries newest first) |
//! | `link_undo` | `entryId: u64` | the new `ActivityEntry` |
//! | `link_set_automation_paused` | `paused: bool` | `LinkSnapshot` |
use crate::link::activity::ActivityEntry;
use crate::link::handle::{ActivitySnapshot, LinkRuntime, LinkSnapshot};
use crate::link::rules::Rule;
use tauri::State;

#[tauri::command]
pub fn link_get_config(link: State<'_, LinkRuntime>) -> LinkSnapshot {
    link.snapshot()
}

#[tauri::command]
pub fn link_set_enabled(link: State<'_, LinkRuntime>, enabled: bool) -> Result<LinkSnapshot, String> {
    link.set_enabled(enabled)
}

#[tauri::command]
pub fn link_set_rules(link: State<'_, LinkRuntime>, rules: Vec<Rule>) -> Result<Vec<Rule>, String> {
    link.set_rules(rules)
}

#[tauri::command]
pub fn link_regenerate_token(link: State<'_, LinkRuntime>) -> Result<LinkSnapshot, String> {
    link.regenerate_token()
}

#[tauri::command]
pub fn link_activity(link: State<'_, LinkRuntime>) -> ActivitySnapshot {
    link.activity()
}

#[tauri::command]
pub fn link_undo(link: State<'_, LinkRuntime>, entry_id: u64) -> Result<ActivityEntry, String> {
    link.undo(entry_id)
}

#[tauri::command]
pub fn link_set_automation_paused(link: State<'_, LinkRuntime>, paused: bool) -> LinkSnapshot {
    link.set_automation_paused(paused)
}

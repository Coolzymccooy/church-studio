//! The Link activity log: a ring of the last `ACTIVITY_CAPACITY` entries,
//! newest first when listed. Pure (the caller supplies timestamps).
use serde::Serialize;
use std::collections::VecDeque;

pub const ACTIVITY_CAPACITY: usize = 50;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntry {
    pub id: u64,
    /// Unix time, milliseconds.
    pub ts: u64,
    pub event: String,
    pub content_kind: Option<String>,
    /// e.g. "loaded Worship", "skipped: scene 'Choir' not found".
    pub action: String,
    /// The scene that was loaded before this entry's change.
    pub previous_scene: Option<String>,
    /// The scene this entry loaded, if any.
    pub loaded_scene: Option<String>,
    /// True when Undo can reload `previous_scene`.
    pub undoable: bool,
}

/// Fields of a new entry (the log assigns `id`).
#[derive(Debug, Clone, Default)]
pub struct NewEntry {
    pub ts: u64,
    pub event: String,
    pub content_kind: Option<String>,
    pub action: String,
    pub previous_scene: Option<String>,
    pub loaded_scene: Option<String>,
}

#[derive(Debug)]
pub struct ActivityLog {
    entries: VecDeque<ActivityEntry>,
    next_id: u64,
    capacity: usize,
}

impl Default for ActivityLog {
    fn default() -> Self {
        Self::new(ACTIVITY_CAPACITY)
    }
}

impl ActivityLog {
    pub fn new(capacity: usize) -> Self {
        ActivityLog {
            entries: VecDeque::with_capacity(capacity),
            next_id: 1,
            capacity: capacity.max(1),
        }
    }

    pub fn push(&mut self, entry: NewEntry) -> ActivityEntry {
        let undoable = entry.loaded_scene.is_some() && entry.previous_scene.is_some();
        let stored = ActivityEntry {
            id: self.next_id,
            ts: entry.ts,
            event: entry.event,
            content_kind: entry.content_kind,
            action: entry.action,
            previous_scene: entry.previous_scene,
            loaded_scene: entry.loaded_scene,
            undoable,
        };
        self.next_id += 1;
        while self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(stored.clone());
        stored
    }

    /// Newest first.
    pub fn list(&self) -> Vec<ActivityEntry> {
        self.entries.iter().rev().cloned().collect()
    }

    pub fn find(&self, id: u64) -> Option<ActivityEntry> {
        self.entries.iter().find(|e| e.id == id).cloned()
    }

    /// The most recent entry's action (used to avoid repeating warnings).
    pub fn last_action(&self) -> Option<&str> {
        self.entries.back().map(|e| e.action.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(action: &str) -> NewEntry {
        NewEntry {
            ts: 1,
            event: "lumina.slide.changed".into(),
            action: action.into(),
            ..NewEntry::default()
        }
    }

    #[test]
    fn keeps_last_capacity_entries_newest_first() {
        let mut log = ActivityLog::new(3);
        for i in 0..5 {
            log.push(entry(&format!("a{i}")));
        }
        let listed: Vec<String> = log.list().into_iter().map(|e| e.action).collect();
        assert_eq!(listed, vec!["a4", "a3", "a2"]);
        assert!(log.find(1).is_none());
        assert_eq!(log.find(5).unwrap().action, "a4");
        assert_eq!(log.last_action(), Some("a4"));
    }

    #[test]
    fn undoable_needs_both_scenes() {
        let mut log = ActivityLog::default();
        let plain = log.push(entry("skipped"));
        assert!(!plain.undoable);
        let loaded = log.push(NewEntry {
            loaded_scene: Some("Worship".into()),
            previous_scene: Some("Sermon".into()),
            ..entry("loaded Worship")
        });
        assert!(loaded.undoable);
        let first_load = log.push(NewEntry {
            loaded_scene: Some("Worship".into()),
            ..entry("loaded Worship")
        });
        assert!(!first_load.undoable);
    }
}

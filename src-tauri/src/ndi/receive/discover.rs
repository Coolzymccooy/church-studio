//! List the NDI® sources on the network (`ndi_list_sources`).
//!
//! A finder is created, given a moment to hear the network, read and
//! destroyed. This blocks for up to `MAX_WAIT`, so the command runs it on
//! the blocking thread pool, never on the UI thread.
use crate::ndi::ffi_recv::FindInstance;
use crate::ndi::runtime::NdiRuntime;
use serde::Serialize;
use std::time::{Duration, Instant};

/// Stop listening once nothing changed for one poll after this long…
const MIN_WAIT: Duration = Duration::from_millis(1_000);
/// …and never listen longer than this.
const MAX_WAIT: Duration = Duration::from_millis(3_000);
const POLL_MS: u32 = 500;

/// One source as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NdiSourceInfo {
    /// Full name, "MACHINE (Source)": what is saved and connected to.
    pub name: String,
    /// Network address, when the SDK reports one.
    pub url: Option<String>,
    /// One of this app's own outputs (receiving it would feed back).
    pub own: bool,
}

/// Dedupe by name (first wins), drop blank names, sort case-insensitively
/// and flag our own outputs.
pub fn tidy_sources(
    found: Vec<(String, Option<String>)>,
    local_machines: &[String],
    own_outputs: &[String],
) -> Vec<NdiSourceInfo> {
    let mut sources: Vec<NdiSourceInfo> = Vec::with_capacity(found.len());
    for (name, url) in found {
        let name = name.trim().to_string();
        if name.is_empty() || sources.iter().any(|s| s.name == name) {
            continue;
        }
        let own = super::is_own_source(&name, local_machines, own_outputs);
        sources.push(NdiSourceInfo { name, url, own });
    }
    sources.sort_by_key(|s| s.name.to_lowercase());
    sources
}

/// Destroys the finder however `list_sources` returns.
struct FinderGuard<'a> {
    runtime: &'a NdiRuntime,
    instance: FindInstance,
}

impl Drop for FinderGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: created by `find_create` and used only in `list_sources`.
        unsafe { self.runtime.find_destroy(self.instance) };
    }
}

/// Listen for sources and return them. Blocking (up to `MAX_WAIT`).
pub fn list_sources(runtime: &NdiRuntime, own_outputs: &[String]) -> Result<Vec<NdiSourceInfo>, String> {
    let finder = FinderGuard {
        runtime,
        instance: runtime.find_create()?,
    };
    let started = Instant::now();
    loop {
        // SAFETY: the finder is live until `finder` is dropped.
        let changed = unsafe { runtime.find_wait(finder.instance, POLL_MS) };
        let waited = started.elapsed();
        if waited >= MAX_WAIT || (!changed && waited >= MIN_WAIT) {
            break;
        }
    }
    // SAFETY: as above; the list is copied before the finder is destroyed.
    let found = unsafe { runtime.find_sources(finder.instance) };
    Ok(tidy_sources(found, &super::local_machine_names(), own_outputs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_are_deduped_sorted_and_flagged() {
        let found = vec![
            ("PC (Zed)".to_string(), None),
            ("  ".to_string(), None),
            ("pc (alpha)".to_string(), Some("10.0.0.2:5961".to_string())),
            ("PC (Zed)".to_string(), None),
            ("PC (TIWATON Studio (Stream))".to_string(), None),
            ("OTHER (TIWATON Studio (Stream))".to_string(), None),
        ];
        let own = vec!["TIWATON Studio (Stream)".to_string()];
        let sources = tidy_sources(found, &["pc".to_string()], &own);
        let names: Vec<&str> = sources.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "OTHER (TIWATON Studio (Stream))",
                "pc (alpha)",
                "PC (TIWATON Studio (Stream))",
                "PC (Zed)",
            ]
        );
        // The same output name on another machine is not ours.
        assert!(!sources[0].own);
        assert_eq!(sources[1].url.as_deref(), Some("10.0.0.2:5961"));
        assert!(sources[2].own);
        assert!(!sources[3].own);
    }
}

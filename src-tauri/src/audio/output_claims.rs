//! Which output device each bus has claimed at engine start, so a monitor
//! mix (mon 2, mon 3) is never opened on a device another output already
//! plays on (`docs/specs/2026-11-20-monitor-mixes.md`).
//!
//! Pure: no cpal, so the duplicate check can be unit-tested.

/// One opened output: what the operator calls it, the selection it was
/// opened from (`None` for the default device) and the resolved device name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ClaimedOutput {
    pub label: &'static str,
    pub selection: Option<String>,
    pub name: String,
}

/// The outputs opened so far, in opening order.
#[derive(Debug, Default)]
pub(super) struct OutputClaims {
    claimed: Vec<ClaimedOutput>,
}

impl OutputClaims {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn claim(&mut self, label: &'static str, selection: Option<&String>, name: &str) {
        self.claimed.push(ClaimedOutput {
            label,
            selection: selection.cloned(),
            name: name.to_string(),
        });
    }

    /// The output already using `selection` (matched against the other
    /// outputs' selections and device names), if any.
    pub fn owner_of_selection(&self, selection: &str) -> Option<&ClaimedOutput> {
        self.claimed.iter().find(|claimed| {
            claimed.selection.as_deref() == Some(selection) || claimed.name == selection
        })
    }

    /// The output already playing on the device named `name`, if any.
    pub fn owner_of_name(&self, name: &str) -> Option<&ClaimedOutput> {
        self.claimed.iter().find(|claimed| claimed.name == name)
    }

    /// Every claimed device name, in opening order.
    pub fn names(&self) -> Vec<String> {
        self.claimed.iter().map(|claimed| claimed.name.clone()).collect()
    }
}

/// The error for `label` choosing `device`, which `owner` already plays on.
pub(super) fn duplicate_output_error(label: &str, device: &str, owner: &ClaimedOutput) -> String {
    format!(
        "The {label} and the {} are both set to '{device}'. Choose a different device for the {label}.",
        owner.label
    )
}

/// Refuse a selection that names a device another output already uses.
/// `Ok(())` when nothing is selected or the selection is free.
pub(super) fn check_selection_free(
    claims: &OutputClaims,
    label: &str,
    selection: Option<&String>,
) -> Result<(), String> {
    match selection.and_then(|selected| claims.owner_of_selection(selected).map(|o| (selected, o)))
    {
        Some((selected, owner)) => Err(duplicate_output_error(label, selected, owner)),
        None => Ok(()),
    }
}

/// Refuse a resolved device name another output already plays on.
pub(super) fn check_name_free(claims: &OutputClaims, label: &str, name: &str) -> Result<(), String> {
    match claims.owner_of_name(name) {
        Some(owner) => Err(duplicate_output_error(label, name, owner)),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims() -> OutputClaims {
        let mut claims = OutputClaims::new();
        // Monitor on the default device (no selection), then the main PA.
        claims.claim("monitor output", None, "MacBook Speakers");
        let pa = "Behringer UMC".to_string();
        claims.claim("main output", Some(&pa), "Behringer UMC");
        claims
    }

    #[test]
    fn a_free_device_or_no_selection_is_accepted() {
        let claims = claims();
        let free = "AirPods".to_string();
        assert_eq!(check_selection_free(&claims, "monitor 2 output", Some(&free)), Ok(()));
        assert_eq!(check_selection_free(&claims, "monitor 2 output", None), Ok(()));
        assert_eq!(check_name_free(&claims, "monitor 2 output", "AirPods"), Ok(()));
    }

    #[test]
    fn a_duplicate_selection_is_refused_naming_both_outputs() {
        let claims = claims();
        let pa = "Behringer UMC".to_string();
        let err = check_selection_free(&claims, "monitor 2 output", Some(&pa)).unwrap_err();
        assert!(err.contains("monitor 2 output"), "{err}");
        assert!(err.contains("main output"), "{err}");
        assert!(err.contains("Behringer UMC"), "{err}");
    }

    #[test]
    fn the_default_monitor_device_is_matched_by_name() {
        let claims = claims();
        let speakers = "MacBook Speakers".to_string();
        let err = check_selection_free(&claims, "monitor 3 output", Some(&speakers)).unwrap_err();
        assert!(err.contains("monitor 3 output") && err.contains("monitor output"), "{err}");
        let err = check_name_free(&claims, "monitor 3 output", "MacBook Speakers").unwrap_err();
        assert!(err.contains("monitor output"), "{err}");
    }

    #[test]
    fn mon2_and_mon3_cannot_share_a_device() {
        let mut claims = claims();
        let phones = "Pastor IEM".to_string();
        assert_eq!(check_selection_free(&claims, "monitor 2 output", Some(&phones)), Ok(()));
        claims.claim("monitor 2 output", Some(&phones), "Pastor IEM");
        let err = check_selection_free(&claims, "monitor 3 output", Some(&phones)).unwrap_err();
        assert!(err.contains("monitor 3 output") && err.contains("monitor 2 output"), "{err}");
        assert_eq!(
            claims.names(),
            vec!["MacBook Speakers", "Behringer UMC", "Pastor IEM"]
        );
    }
}

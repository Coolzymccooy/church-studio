//! Which output device each bus plays on, and on which pair
//! (`docs/specs/2026-11-20-usb-interfaces.md` §4, after
//! `2026-11-20-monitor-mixes.md`).
//!
//! One group per device: the buses that play on it, each on its own pair.
//! Pure (no cpal), so the sharing and duplicate rules are unit-tested.

/// One bus asking for a device. `name` is the device's unique name (the
/// resolved default for the monitor); `present` is false when it is not
/// connected now.
#[derive(Debug, Clone)]
pub struct BusRequest {
    pub bus: usize,
    /// What the operator calls it ("monitor 2 output").
    pub role: &'static str,
    pub name: String,
    pub present: bool,
    /// 0 = channels 1-2, 1 = 3-4, …
    pub pair: usize,
    /// A taken device pair is an error (mon 2 / mon 3) rather than a skip
    /// (broadcast, main).
    pub exclusive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Member {
    pub bus: usize,
    pub pair: usize,
}

/// One output device and the buses on it, in request order.
#[derive(Debug, Clone)]
pub struct PlannedGroup {
    pub name: String,
    pub present: bool,
    pub members: Vec<Member>,
    roles: Vec<&'static str>,
}

impl PlannedGroup {
    /// Samples per ring frame: (L, R) per bus.
    pub fn width(&self) -> usize {
        self.members.len() * 2
    }

    pub fn pairs(&self) -> Vec<usize> {
        self.members.iter().map(|m| m.pair).collect()
    }
}

#[derive(Debug, Clone, Default)]
pub struct OutputPlan {
    pub groups: Vec<PlannedGroup>,
    /// Buses not played because their device pair was already taken.
    pub skipped: Vec<usize>,
}

impl OutputPlan {
    /// Names of the devices connected now.
    #[cfg(test)]
    pub fn present_names(&self) -> Vec<String> {
        self.groups
            .iter()
            .filter(|g| g.present)
            .map(|g| g.name.clone())
            .collect()
    }

    /// The device a bus plays on, if it was planned.
    pub fn device_of(&self, bus: usize) -> Option<&str> {
        self.groups
            .iter()
            .find(|g| g.members.iter().any(|m| m.bus == bus))
            .map(|g| g.name.as_str())
    }
}

/// "1-2", "3-4", … for pair index `pair`.
pub fn pair_label(pair: usize) -> String {
    format!("{}-{}", pair * 2 + 1, pair * 2 + 2)
}

fn duplicate_error(role: &str, device: &str, pair: usize, owner: &str) -> String {
    format!(
        "The {role} and the {owner} are both set to '{device}' {}. Choose a different device or pair for the {role}.",
        pair_label(pair)
    )
}

/// Group the requests by device, in order. A request whose device pair is
/// taken is skipped, or refused when `exclusive`.
pub fn plan_outputs(requests: &[BusRequest]) -> Result<OutputPlan, String> {
    let mut plan = OutputPlan::default();
    for request in requests {
        let Some(group) = plan.groups.iter_mut().find(|g| g.name == request.name) else {
            plan.groups.push(PlannedGroup {
                name: request.name.clone(),
                present: request.present,
                members: vec![Member { bus: request.bus, pair: request.pair }],
                roles: vec![request.role],
            });
            continue;
        };
        if let Some(taken) = group.members.iter().position(|m| m.pair == request.pair) {
            if request.exclusive {
                return Err(duplicate_error(
                    request.role,
                    &request.name,
                    request.pair,
                    group.roles[taken],
                ));
            }
            plan.skipped.push(request.bus);
            continue;
        }
        group.members.push(Member { bus: request.bus, pair: request.pair });
        group.roles.push(request.role);
    }
    Ok(plan)
}

//! What each engine stream is connected to and how it is doing, for
//! `engine_status.devices`, the `audio-device-health` event and the device
//! watcher (`docs/specs/2026-11-20-usb-interfaces.md` §3).
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use super::health::{LinkState, StreamHealth};
use super::output_feed::{ClockMode, OutputStats};
use super::output_plan::{pair_label, Member};
use crate::mixer_control::BUS_IDS;

/// The input stream.
#[derive(Debug)]
pub struct InputLink {
    pub name: String,
    pub health: Arc<StreamHealth>,
    /// First device channel read (0-based) and the strips it feeds.
    pub first: usize,
    pub count: usize,
    pub channels: AtomicUsize,
}

/// One output device and the buses on it.
#[derive(Debug)]
pub struct OutputLink {
    pub name: String,
    pub members: Vec<Member>,
    pub clock: ClockMode,
    pub health: Arc<StreamHealth>,
    pub stats: Arc<OutputStats>,
    /// Members whose pair is not on the opened device.
    unfit: Mutex<Vec<usize>>,
    pub channels: AtomicUsize,
}

impl OutputLink {
    pub fn new(name: String, members: Vec<Member>, clock: ClockMode, state: LinkState) -> Self {
        OutputLink {
            name,
            members,
            clock,
            health: Arc::new(StreamHealth::new(state)),
            stats: Arc::new(OutputStats::default()),
            unfit: Mutex::new(Vec::new()),
            channels: AtomicUsize::new(0),
        }
    }

    /// Record the opened device's channels and the members that do not fit.
    pub fn opened(&self, channels: usize, unfit: Vec<usize>) {
        self.channels.store(channels, Ordering::Relaxed);
        *self.unfit.lock() = unfit;
    }

    fn member_json(&self, index: usize, member: &Member) -> Value {
        let channels = self.channels.load(Ordering::Relaxed);
        let unfit = self.unfit.lock().contains(&index);
        let state = self.health.state();
        let (state, error) = if unfit && state == LinkState::Ok {
            (
                LinkState::Failed,
                Some(format!(
                    "pair {} is not on this device ({channels} channels)",
                    pair_label(member.pair)
                )),
            )
        } else {
            (state, self.health.error())
        };
        json!({
            "role": BUS_IDS.get(member.bus).copied().unwrap_or("bus"),
            "name": self.name,
            "state": state.word(),
            "error": error,
            "pair": member.pair,
            "channels": channels,
            "clock": self.clock.word(),
            "drift_ppm": self.stats.drift_ppm(),
            "underruns": self.stats.underruns(),
        })
    }
}

/// Every stream of one running engine.
#[derive(Debug)]
pub struct EngineLinks {
    pub input: InputLink,
    pub outputs: Vec<OutputLink>,
}

impl EngineLinks {
    /// (stream index, device name, is input, health): 0 is the input,
    /// i + 1 is output device i.
    pub fn streams(&self) -> Vec<(usize, &str, bool, &StreamHealth)> {
        let mut list = vec![(0, self.input.name.as_str(), true, self.input.health.as_ref())];
        list.extend(
            self.outputs
                .iter()
                .enumerate()
                .map(|(i, o)| (i + 1, o.name.as_str(), false, o.health.as_ref())),
        );
        list
    }

    /// One entry per role, input first (`engine_status.devices`).
    pub fn status_json(&self) -> Value {
        let input = &self.input;
        let mut list = vec![json!({
            "role": "input",
            "name": input.name,
            "state": input.health.state().word(),
            "error": input.health.error(),
            "first": input.first,
            "count": input.count,
            "channels": input.channels.load(Ordering::Relaxed),
            "clock": "shared",
            "drift_ppm": 0,
            "underruns": 0,
        })];
        for output in &self.outputs {
            for (index, member) in output.members.iter().enumerate() {
                list.push(output.member_json(index, member));
            }
        }
        Value::Array(list)
    }

    /// The link states, to notice a change.
    pub fn states(&self) -> Vec<LinkState> {
        self.streams().iter().map(|(_, _, _, h)| h.state()).collect()
    }
}

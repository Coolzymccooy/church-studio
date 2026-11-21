//! `start_audio_engine` arguments for output pairs and the input channel
//! range (`docs/specs/2026-11-20-usb-interfaces.md` §4). Validated here, at
//! the boundary: a bad value falls back to the safe default.
use crate::dsp::mixer::{BUS_MAIN, BUS_MONITOR, BUS_MONITOR2, BUS_MONITOR3, BUS_STREAM, NUM_BUSES};
use serde::Deserialize;

/// Highest pair index accepted (channels 63-64).
pub const MAX_PAIR: usize = 31;
/// Highest first input channel accepted (0-based).
pub const MAX_INPUT_CHANNEL: usize = 255;

/// `outputPairs: { main, stream, monitor, monitor2, monitor3 }`; a missing
/// key is pair 0 (channels 1-2).
#[derive(Debug, Default, Clone, Deserialize)]
pub struct OutputPairsArg {
    pub main: Option<usize>,
    pub stream: Option<usize>,
    pub monitor: Option<usize>,
    pub monitor2: Option<usize>,
    pub monitor3: Option<usize>,
}

impl OutputPairsArg {
    /// Pairs in `BUS_*` order; anything above `MAX_PAIR` becomes 0.
    pub fn to_buses(&self) -> [usize; NUM_BUSES] {
        let clean = |pair: Option<usize>| pair.filter(|p| *p <= MAX_PAIR).unwrap_or(0);
        let mut pairs = [0; NUM_BUSES];
        pairs[BUS_MAIN] = clean(self.main);
        pairs[BUS_STREAM] = clean(self.stream);
        pairs[BUS_MONITOR] = clean(self.monitor);
        pairs[BUS_MONITOR2] = clean(self.monitor2);
        pairs[BUS_MONITOR3] = clean(self.monitor3);
        pairs
    }
}

/// `inputChannels: { first, count }`: 0-based first channel, `count` null
/// for every channel from there.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct InputChannelsArg {
    pub first: Option<usize>,
    pub count: Option<usize>,
}

impl InputChannelsArg {
    /// `None` (every channel) for the default range or a first channel out
    /// of bounds.
    pub fn to_range(&self) -> Option<(usize, Option<usize>)> {
        let first = self.first.unwrap_or(0);
        if first > MAX_INPUT_CHANNEL {
            return None;
        }
        if first == 0 && self.count.is_none() {
            return None;
        }
        Some((first, self.count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_land_in_bus_order_and_bad_values_fall_back() {
        let arg: OutputPairsArg =
            serde_json::from_str(r#"{"main":0,"stream":1,"monitor2":99}"#).unwrap();
        let pairs = arg.to_buses();
        assert_eq!(pairs[BUS_MAIN], 0);
        assert_eq!(pairs[BUS_STREAM], 1);
        assert_eq!(pairs[BUS_MONITOR], 0);
        assert_eq!(pairs[BUS_MONITOR2], 0);
        assert_eq!(pairs[BUS_MONITOR3], 0);
    }

    #[test]
    fn input_range_defaults_to_every_channel() {
        let all: InputChannelsArg = serde_json::from_str(r#"{"first":0,"count":null}"#).unwrap();
        assert_eq!(all.to_range(), None);
        let some: InputChannelsArg = serde_json::from_str(r#"{"first":2,"count":4}"#).unwrap();
        assert_eq!(some.to_range(), Some((2, Some(4))));
        let first8: InputChannelsArg = serde_json::from_str(r#"{"first":0,"count":8}"#).unwrap();
        assert_eq!(first8.to_range(), Some((0, Some(8))));
        let wild: InputChannelsArg = serde_json::from_str(r#"{"first":9999}"#).unwrap();
        assert_eq!(wild.to_range(), None);
    }
}

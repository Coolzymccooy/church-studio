//! NDI® audio input (design decision 5 in
//! `docs/specs/2026-09-28-integrations-design.md`).
//!
//! - `settings`: which sources are received (`ndi_inputs.json`).
//! - `discover`: list the sources on the network (`ndi_list_sources`).
//! - `receiver`: one capture thread per source, max `MAX_NDI_INPUTS`.
//! - `convert`: received FLTP → interleaved stereo at the engine rate.
//! - `drift`: the audio callback's ring read, with clock-drift correction.
//!
//! Each received source becomes one extra mixer strip after the hardware
//! channels (`crate::mixer_layout`), fed with the source's (L + R) / 2.
//! Settings apply at the next engine start, like NDI out.
pub mod convert;
pub mod discover;
pub mod drift;
pub mod receiver;
pub mod settings;

pub use settings::{NdiInputSettings, NdiInputs, MAX_NDI_INPUTS};

use crate::mixer_control::MAX_STRIP_NAME_CHARS;
use drift::DriftReader;
use receiver::NdiReceiver;
use ringbuf::traits::{Consumer, Split};
use ringbuf::{HeapCons, HeapRb};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Ring capacity: one second of stereo at the engine rate.
pub fn ring_capacity(sample_rate: u32) -> usize {
    (sample_rate as usize * convert::STEREO).max(4_096)
}

/// Live counters of one input. The receiver thread and the audio callback
/// write them with relaxed atomics; `engine_status` reads them.
#[derive(Debug, Default)]
pub struct NdiInputStats {
    /// Audio arrived within the last second.
    pub connected: AtomicBool,
    /// Stereo frames waiting in the ring (audio callback).
    pub fill_frames: AtomicU64,
    /// Samples lost: ring full (receiver) or trimmed on overrun (callback).
    pub dropped_samples: AtomicU64,
    /// Times the ring ran dry during playback (callback).
    pub underruns: AtomicU64,
    /// Frames that were not planar float or could not be read safely.
    pub skipped_frames: AtomicU64,
}

/// The audio callback's side of one input: its ring and drift reader.
pub struct NdiInputFeed<C> {
    consumer: C,
    reader: DriftReader,
    stats: Arc<NdiInputStats>,
}

impl<C: Consumer<Item = f32>> NdiInputFeed<C> {
    pub fn new(consumer: C, sample_rate: u32, stats: Arc<NdiInputStats>) -> Self {
        NdiInputFeed {
            consumer,
            reader: DriftReader::new(sample_rate),
            stats,
        }
    }

    /// Audio thread: fill `out` with the next block of this source (mono).
    /// Real-time safe: atomics and ring operations only.
    pub fn pull(&mut self, out: &mut [f32]) {
        let report = self.reader.read(&mut self.consumer, out);
        self.stats
            .fill_frames
            .store(report.fill_frames as u64, Ordering::Relaxed);
        if report.dropped_samples > 0 {
            self.stats
                .dropped_samples
                .fetch_add(report.dropped_samples as u64, Ordering::Relaxed);
        }
        if report.underrun {
            self.stats.underruns.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// A started input, for status and strip naming.
#[derive(Debug, Clone)]
pub struct NdiInputHandle {
    /// The full NDI source name.
    pub source: String,
    /// The mixer strip it feeds.
    pub strip: usize,
    /// Default strip name, from the source name.
    pub label: String,
    pub stats: Arc<NdiInputStats>,
}

/// Default strip name for input `slot`'s `source`: the part inside the
/// parentheses of "MACHINE (Source)" (or the whole name), trimmed, without
/// control characters and cut to the strip-name limit. Empty → "Net n".
pub fn strip_label(source: &str, slot: usize) -> String {
    let trimmed = source.trim();
    let inner = match (trimmed.find(" ("), trimmed.ends_with(')')) {
        (Some(open), true) => &trimmed[open + 2..trimmed.len() - 1],
        _ => trimmed,
    };
    let label: String = inner
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_STRIP_NAME_CHARS)
        .collect();
    let label = label.trim();
    if label.is_empty() {
        format!("Net {}", slot + 1)
    } else {
        label.to_string()
    }
}

/// `source` is one of this app's own NDI outputs (`own_outputs` are our
/// source names, e.g. "TIWATON Studio (Stream)"). Receiving it would feed
/// the mix back into itself.
pub fn is_own_source(source: &str, own_outputs: &[String]) -> bool {
    own_outputs
        .iter()
        .any(|own| source.trim().ends_with(&format!("({own})")))
}

/// What `start_receivers` hands the engine: the feeds for the audio
/// callback (in strip order), the receiver threads and their handles.
pub struct StartedInputs {
    pub feeds: Vec<NdiInputFeed<HeapCons<f32>>>,
    pub receivers: Vec<NdiReceiver>,
    pub handles: Vec<NdiInputHandle>,
}

/// Start one receiver per selected source, at most `max_inputs`, feeding
/// strips `first_strip`, `first_strip + 1`, …. A source that cannot start is
/// logged and skipped, so the engine always starts.
pub fn start_receivers(
    inputs: &NdiInputs,
    sample_rate: u32,
    first_strip: usize,
    max_inputs: usize,
) -> StartedInputs {
    let mut started = StartedInputs {
        feeds: Vec::new(),
        receivers: Vec::new(),
        handles: Vec::new(),
    };
    let sources = settings::sanitize_sources(&inputs.sources);
    if sources.is_empty() {
        return started;
    }
    if sources.len() > max_inputs {
        log::warn!(
            "only {max_inputs} of {} NDI inputs fit in the mixer",
            sources.len()
        );
    }
    let runtime = match crate::ndi::runtime::runtime() {
        Ok(runtime) if runtime.supports_receive() => runtime,
        Ok(_) => {
            log::warn!("NDI inputs not started: this NDI Runtime cannot receive");
            return started;
        }
        Err(err) => {
            log::warn!("NDI inputs not started: {err}");
            return started;
        }
    };
    for source in sources.into_iter().take(max_inputs) {
        let slot = started.handles.len();
        let stats = Arc::new(NdiInputStats::default());
        let (producer, consumer) = HeapRb::<f32>::new(ring_capacity(sample_rate)).split();
        match NdiReceiver::start(runtime, &source, sample_rate, producer, stats.clone()) {
            Ok(receiver) => {
                log::info!("NDI input '{source}' receiving");
                started.handles.push(NdiInputHandle {
                    strip: first_strip + slot,
                    label: strip_label(&source, slot),
                    source,
                    stats: stats.clone(),
                });
                started.feeds.push(NdiInputFeed::new(consumer, sample_rate, stats));
                started.receivers.push(receiver);
            }
            Err(err) => log::warn!("NDI input '{source}' not started: {err}"),
        }
    }
    started
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::traits::Producer;

    #[test]
    fn labels_come_from_the_source_part_of_the_name() {
        assert_eq!(strip_label("CHURCH-PC (Keys)", 0), "Keys");
        assert_eq!(
            strip_label("PC (TIWATON Studio (Stream))", 0),
            "TIWATON Studio (Stream)"
        );
        assert_eq!(strip_label("  Plain name ", 1), "Plain name");
        assert_eq!(strip_label("PC ()", 2), "Net 3");
        assert_eq!(strip_label("", 0), "Net 1");
        let long = format!("PC ({})", "x".repeat(40));
        assert_eq!(strip_label(&long, 0).chars().count(), MAX_STRIP_NAME_CHARS);
    }

    #[test]
    fn own_outputs_are_recognised() {
        let own = vec!["TIWATON Studio (Stream)".to_string()];
        assert!(is_own_source("CHURCH-PC (TIWATON Studio (Stream))", &own));
        assert!(!is_own_source("CHURCH-PC (TIWATON Studio (Main))", &own));
        assert!(!is_own_source("CHURCH-PC (Keys)", &[]));
    }

    #[test]
    fn nothing_selected_starts_nothing_without_the_runtime() {
        let started = start_receivers(&NdiInputs::default(), 48_000, 2, 4);
        assert!(started.feeds.is_empty() && started.handles.is_empty());
    }

    #[test]
    fn feed_updates_the_counters() {
        let (mut producer, consumer) = HeapRb::<f32>::new(ring_capacity(48_000)).split();
        let stats = Arc::new(NdiInputStats::default());
        let mut feed = NdiInputFeed::new(consumer, 48_000, stats.clone());
        let mut out = vec![0.0f32; 256];
        feed.pull(&mut out);
        assert_eq!(stats.fill_frames.load(Ordering::Relaxed), 0);
        for _ in 0..4_000 {
            producer.try_push(0.25).unwrap();
            producer.try_push(0.25).unwrap();
        }
        feed.pull(&mut out);
        assert_eq!(stats.fill_frames.load(Ordering::Relaxed), 4_000);
        assert!(out.iter().all(|&s| (s - 0.25).abs() < 1e-6));
        assert_eq!(stats.underruns.load(Ordering::Relaxed), 0);
    }
}

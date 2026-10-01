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

/// Names this machine may appear under as the MACHINE part of an NDI
/// source name: the Windows computer name and the host name.
pub fn local_machine_names() -> Vec<String> {
    let mut names = Vec::with_capacity(2);
    if let Ok(name) = std::env::var("COMPUTERNAME") {
        names.push(name);
    }
    names.push(gethostname::gethostname().to_string_lossy().into_owned());
    names.retain(|name| !name.trim().is_empty());
    names
}

/// "MACHINE (Source)" → ("MACHINE", "Source"). The machine name runs to the
/// first " (" (host names have no spaces); the source is everything inside
/// the outer parentheses, so it may contain parentheses itself.
pub fn split_source_name(full: &str) -> Option<(&str, &str)> {
    let trimmed = full.trim();
    let open = trimmed.find(" (")?;
    let source = trimmed[open + 2..].strip_suffix(')')?;
    Some((trimmed[..open].trim(), source))
}

/// `machine` is `host`, without case; a host name's first DNS label counts
/// too ("studio-pc.local" is "STUDIO-PC").
fn same_machine(machine: &str, host: &str) -> bool {
    let host = host.trim();
    let short = host.split('.').next().unwrap_or(host);
    let machine = machine.to_lowercase();
    machine == host.to_lowercase() || machine == short.to_lowercase()
}

/// `source` is one of this app's own NDI outputs: its machine part is one of
/// `local_machines` (`local_machine_names`) and its source part is one of
/// `own_outputs` (our enabled output names, e.g. "TIWATON Studio (Stream)").
/// Receiving it would feed the mix back into itself. The same name from
/// another machine is someone else's output and is allowed.
pub fn is_own_source(source: &str, local_machines: &[String], own_outputs: &[String]) -> bool {
    let Some((machine, name)) = split_source_name(source) else {
        return false;
    };
    local_machines.iter().any(|host| same_machine(machine, host))
        && own_outputs.iter().any(|own| own == name)
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
    fn source_names_split_into_machine_and_source() {
        assert_eq!(split_source_name("PC (Keys)"), Some(("PC", "Keys")));
        assert_eq!(
            split_source_name(" PC (TIWATON Studio (Stream)) "),
            Some(("PC", "TIWATON Studio (Stream)"))
        );
        assert_eq!(split_source_name("Plain name"), None);
        assert_eq!(split_source_name("PC (unclosed"), None);
    }

    #[test]
    fn own_outputs_are_recognised_on_this_machine_only() {
        let own = vec!["TIWATON Studio (Stream)".to_string()];
        let here = vec!["CHURCH-PC".to_string()];
        // Same host, any case, source name with parentheses.
        assert!(is_own_source("CHURCH-PC (TIWATON Studio (Stream))", &here, &own));
        assert!(is_own_source("church-pc (TIWATON Studio (Stream))", &here, &own));
        let fqdn = vec!["church-pc.local".to_string()];
        assert!(is_own_source("CHURCH-PC (TIWATON Studio (Stream))", &fqdn, &own));
        // Another machine's default output is someone else's.
        assert!(!is_own_source("OTHER-PC (TIWATON Studio (Stream))", &here, &own));
        // Our machine, but not one of our enabled outputs.
        assert!(!is_own_source("CHURCH-PC (TIWATON Studio (Main))", &here, &own));
        assert!(!is_own_source("CHURCH-PC (Keys)", &here, &[]));
        // No local name known: nothing is ours.
        assert!(!is_own_source("CHURCH-PC (TIWATON Studio (Stream))", &[], &own));
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

//! Computer audio in: what this computer plays (a music app, a browser, a
//! video player) as a stereo pair of mixer strips, with no virtual cable,
//! BlackHole or Aggregate Device for the operator to set up.
//!
//! - macOS (`macos`): a Core Audio process tap of every process except this
//!   one, muted at its normal output while the tap runs, so this app is the
//!   only path the sound takes and its own output never feeds back in.
//! - Windows (`windows`): WASAPI loopback of the default output device.
//!   This app's own output cannot be left out there, so the engine refuses
//!   to capture a device it also plays to.
//! - `capture`: platform buffers → stereo at the engine rate → ring.
//! - The audio callback reads the ring with the NDI inputs' drift reader
//!   (`ComputerAudioFeed`), keeping left and right apart.
//!
//! Off by default (`settings`); applied at the next engine start.
pub mod capture;
pub mod settings;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

pub use settings::{ComputerAudio, ComputerAudioSettings};

use crate::ndi::receive::drift::DriftReader;
use crate::ndi::receive::ring_capacity;
use capture::CaptureWriter;
use ringbuf::traits::{Consumer, Split};
use ringbuf::{HeapCons, HeapRb};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Live counters, written with relaxed atomics by the capture thread and
/// the audio callback; read by `engine_status`.
#[derive(Debug, Default)]
pub struct ComputerAudioStats {
    /// Blocks the capture side delivered (rises while capture runs).
    pub captured_blocks: AtomicU64,
    /// Stereo frames waiting in the ring (audio callback).
    pub fill_frames: AtomicU64,
    /// Samples lost: ring full (capture) or trimmed on overrun (callback).
    pub dropped_samples: AtomicU64,
    /// Times the ring ran dry during playback (callback).
    pub underruns: AtomicU64,
}

/// The audio callback's side: the ring and its drift reader.
pub struct ComputerAudioFeed<C> {
    consumer: C,
    reader: DriftReader,
    stats: Arc<ComputerAudioStats>,
}

impl<C: Consumer<Item = f32>> ComputerAudioFeed<C> {
    pub fn new(consumer: C, sample_rate: u32, stats: Arc<ComputerAudioStats>) -> Self {
        ComputerAudioFeed {
            consumer,
            reader: DriftReader::new(sample_rate),
            stats,
        }
    }

    /// Audio thread: fill the left and right strip buffers with the next
    /// block. Real-time safe: atomics and ring operations only.
    pub fn pull(&mut self, left: &mut [f32], right: &mut [f32]) {
        let report = self.reader.read_stereo(&mut self.consumer, left, right);
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

/// A running capture, for status.
#[derive(Debug, Clone)]
pub struct ComputerAudioHandle {
    /// The (left, right) strips it feeds.
    pub strips: (usize, usize),
    /// What is captured, for the status ("All apps", a device name).
    pub source: String,
    /// Rate the platform delivers at (converted to the engine rate).
    pub capture_rate: u32,
    pub stats: Arc<ComputerAudioStats>,
}

/// Platform capture; stops (and on macOS removes the tap) when dropped.
pub struct ComputerAudioCapture {
    #[cfg(target_os = "macos")]
    _inner: macos::ProcessTap,
    #[cfg(target_os = "windows")]
    _inner: windows::LoopbackCapture,
}

/// Where the engine sends its own sound, so the capture can make sure it
/// never records it.
pub struct OwnOutputs<'a> {
    /// Names of the output devices the engine opened (checked on Windows;
    /// macOS leaves this process out of the tap instead).
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub device_names: &'a [String],
}

/// A ring and its feed, made before the audio callback is built. The
/// capture starts later (`PendingCapture::start`), once the engine's own
/// outputs run: on macOS this process can only be left out of the tap
/// once it is a Core Audio client.
pub struct PendingCapture {
    producer: ringbuf::HeapProd<f32>,
    sample_rate: u32,
    strips: (usize, usize),
    stats: Arc<ComputerAudioStats>,
}

pub fn prepare(sample_rate: u32, strips: (usize, usize)) -> (PendingCapture, ComputerAudioFeed<HeapCons<f32>>) {
    let stats = Arc::new(ComputerAudioStats::default());
    let (producer, consumer) = HeapRb::<f32>::new(ring_capacity(sample_rate)).split();
    let feed = ComputerAudioFeed::new(consumer, sample_rate, stats.clone());
    (
        PendingCapture {
            producer,
            sample_rate,
            strips,
            stats,
        },
        feed,
    )
}

impl PendingCapture {
    /// Start capturing. Every failure is an error the engine reports: a
    /// computer-audio pair that is on but silent would look like a fault in
    /// the mixer.
    pub fn start(self, own: &OwnOutputs<'_>) -> Result<(ComputerAudioCapture, ComputerAudioHandle), String> {
        let started = start_platform(self.sample_rate, self.producer, self.stats.clone(), own)?;
        log::info!(
            "computer audio capturing {} at {} Hz into strips {} and {}",
            started.source,
            started.capture_rate,
            self.strips.0 + 1,
            self.strips.1 + 1
        );
        let handle = ComputerAudioHandle {
            strips: self.strips,
            source: started.source,
            capture_rate: started.capture_rate,
            stats: self.stats,
        };
        Ok((started.capture, handle))
    }
}

struct PlatformStart {
    capture: ComputerAudioCapture,
    source: String,
    capture_rate: u32,
}

/// Make the writer for a platform that delivers at `capture_rate`.
#[allow(dead_code)] // unused on platforms without a backend
fn writer(
    producer: ringbuf::HeapProd<f32>,
    capture_rate: u32,
    engine_rate: u32,
    stats: Arc<ComputerAudioStats>,
) -> CaptureWriter<ringbuf::HeapProd<f32>> {
    CaptureWriter::new(producer, capture_rate, engine_rate, stats)
}

#[cfg(target_os = "macos")]
fn start_platform(
    sample_rate: u32,
    producer: ringbuf::HeapProd<f32>,
    stats: Arc<ComputerAudioStats>,
    _own: &OwnOutputs<'_>,
) -> Result<PlatformStart, String> {
    // The tap leaves this process out, so the engine's own outputs never
    // reach it whatever devices they use.
    let tap = macos::ProcessTap::start(|capture_rate| writer(producer, capture_rate, sample_rate, stats))?;
    let capture_rate = tap.capture_rate();
    Ok(PlatformStart {
        capture: ComputerAudioCapture { _inner: tap },
        source: "All apps".to_string(),
        capture_rate,
    })
}

#[cfg(target_os = "windows")]
fn start_platform(
    sample_rate: u32,
    producer: ringbuf::HeapProd<f32>,
    stats: Arc<ComputerAudioStats>,
    own: &OwnOutputs<'_>,
) -> Result<PlatformStart, String> {
    let capture = windows::LoopbackCapture::start(own.device_names, |capture_rate| {
        writer(producer, capture_rate, sample_rate, stats)
    })?;
    let source = capture.device_name().to_string();
    let capture_rate = capture.capture_rate();
    Ok(PlatformStart {
        capture: ComputerAudioCapture { _inner: capture },
        source,
        capture_rate,
    })
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn start_platform(
    _sample_rate: u32,
    _producer: ringbuf::HeapProd<f32>,
    _stats: Arc<ComputerAudioStats>,
    _own: &OwnOutputs<'_>,
) -> Result<PlatformStart, String> {
    Err("Computer audio is available on macOS and Windows only. Turn it off to start the engine.".to_string())
}

/// Permission to record other apps' sound: "granted", "denied" or
/// "unknown" (not asked yet, or the platform does not say). Windows needs
/// none.
pub fn permission() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        macos::permission().as_str()
    }
    #[cfg(not(target_os = "macos"))]
    {
        "granted"
    }
}

/// The `computer_audio` object of `engine_status`.
pub fn status_json(applied: bool, handle: Option<&ComputerAudioHandle>, sample_rate: u32) -> serde_json::Value {
    let Some(handle) = handle else {
        return serde_json::json!({ "enabled": applied, "running": false });
    };
    let stats = &handle.stats;
    let fill = stats.fill_frames.load(Ordering::Relaxed) as f64;
    serde_json::json!({
        "enabled": applied,
        "running": true,
        "source": handle.source,
        "strips": [handle.strips.0, handle.strips.1],
        "capture_rate": handle.capture_rate,
        "captured_blocks": stats.captured_blocks.load(Ordering::Relaxed),
        "fill_ms": (fill * 1000.0 / sample_rate.max(1) as f64).round(),
        "dropped_samples": stats.dropped_samples.load(Ordering::Relaxed),
        "underruns": stats.underruns.load(Ordering::Relaxed),
        "permission": permission(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ndi::receive::drift::{frames_for_ms, TARGET_FILL_MS};
    use ringbuf::traits::Producer;

    #[test]
    fn feed_reads_both_sides_and_updates_the_counters() {
        let (mut producer, consumer) = HeapRb::<f32>::new(ring_capacity(48_000)).split();
        let stats = Arc::new(ComputerAudioStats::default());
        let mut feed = ComputerAudioFeed::new(consumer, 48_000, stats.clone());
        let (mut left, mut right) = (vec![1.0f32; 256], vec![1.0f32; 256]);
        feed.pull(&mut left, &mut right);
        assert!(left.iter().chain(right.iter()).all(|&s| s == 0.0));
        let frames = frames_for_ms(48_000, TARGET_FILL_MS) * 2;
        for _ in 0..frames {
            producer.try_push(0.5).unwrap();
            producer.try_push(-0.25).unwrap();
        }
        feed.pull(&mut left, &mut right);
        assert_eq!(stats.fill_frames.load(Ordering::Relaxed), frames as u64);
        assert!(left.iter().all(|&s| (s - 0.5).abs() < 1e-6));
        assert!(right.iter().all(|&s| (s + 0.25).abs() < 1e-6));
        assert_eq!(stats.underruns.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn status_says_off_or_reports_the_counters() {
        let off = status_json(false, None, 48_000);
        assert_eq!(off["enabled"], false);
        assert_eq!(off["running"], false);
        let handle = ComputerAudioHandle {
            strips: (2, 3),
            source: "All apps".to_string(),
            capture_rate: 44_100,
            stats: Arc::new(ComputerAudioStats::default()),
        };
        handle.stats.fill_frames.store(1_920, Ordering::Relaxed);
        let on = status_json(true, Some(&handle), 48_000);
        assert_eq!(on["running"], true);
        assert_eq!(on["strips"], serde_json::json!([2, 3]));
        assert_eq!(on["fill_ms"], 40.0);
        assert_eq!(on["capture_rate"], 44_100);
    }
}

#[cfg(all(test, target_os = "macos"))]
mod engine_path_probe {
    //! The engine's own path with real clocks: `prepare` → capture start →
    //! `ComputerAudioFeed::pull` in 256-frame blocks driven by the default
    //! input device's callback (as the engine's input callback does), for
    //! 20 s while Apple Music plays.
    //! `cargo test computer_audio_engine_path_probe -- --ignored --nocapture`
    use super::*;
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::Mutex;

    #[test]
    #[ignore = "needs a Mac, an input device, Apple Music playing and the permission"]
    fn computer_audio_engine_path_probe() {
        let host = cpal::default_host();
        let input = host.default_input_device().expect("input");
        let config = input.default_input_config().unwrap();
        let rate = config.sample_rate();
        let channels = config.channels() as usize;
        let (pending, feed) = prepare(rate, (1, 2));
        let peaks = Arc::new(Mutex::new((0.0f32, 0.0f32, 0u64)));
        let stats_peaks = peaks.clone();
        let mut feed = feed;
        let (mut left, mut right) = (vec![0.0f32; 4096], vec![0.0f32; 4096]);
        let stream = input
            .build_input_stream(
                StreamConfig { channels: config.channels(), sample_rate: rate, buffer_size: cpal::BufferSize::Fixed(256) },
                move |data: &[f32], _| {
                    let n = (data.len() / channels).min(4096);
                    feed.pull(&mut left[..n], &mut right[..n]);
                    if let Ok(mut p) = stats_peaks.try_lock() {
                        p.0 = left[..n].iter().fold(p.0, |m, s| m.max(s.abs()));
                        p.1 = right[..n].iter().fold(p.1, |m, s| m.max(s.abs()));
                        p.2 += n as u64;
                    }
                },
                |e| eprintln!("input error: {e}"),
                None,
            )
            .unwrap();
        stream.play().unwrap();
        let names = ["Jabra Engage 50".to_string()];
        let (capture, handle) = pending.start(&OwnOutputs { device_names: &names }).expect("capture");
        std::thread::sleep(std::time::Duration::from_secs(2));
        let underruns_after_start = handle.stats.underruns.load(Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_secs(18));
        drop(stream);
        drop(capture);
        let (l, r, frames) = *peaks.lock().unwrap();
        let s = &handle.stats;
        println!("engine rate {rate} Hz, capture rate {} Hz", handle.capture_rate);
        println!("pulled {frames} frames; peak L {l:.3}, R {r:.3}");
        println!(
            "fill {} frames, dropped {} samples, underruns {} (of which after 2 s: {})",
            s.fill_frames.load(Ordering::Relaxed),
            s.dropped_samples.load(Ordering::Relaxed),
            s.underruns.load(Ordering::Relaxed),
            s.underruns.load(Ordering::Relaxed) - underruns_after_start
        );
        assert!(l > 0.01 && r > 0.01, "no music on the strips");
        assert_eq!(s.underruns.load(Ordering::Relaxed) - underruns_after_start, 0);
    }
    use cpal::StreamConfig;
}

//! The meters thread: logs dropped output samples and emits
//! `audio-meters` and `mixer-meters` (moved from `audio.rs`).
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::{EngineShared, MIXER_METERS_INTERVAL_MS};
use crate::dsp::MetersPayload;
use crate::mixer_meters::{LatestMeters, MixerMeterSlots};

/// Meters thread: logs dropped output samples, emits `audio-meters`
/// (~20 Hz, spectrum averaged over the pending payloads) and `mixer-meters`
/// (~20 Hz, read from the lock-free slots). Exits when the input callback
/// (the only sender) is dropped with the engine.
pub(super) fn spawn_meters_thread<R: Runtime>(
    app: AppHandle<R>,
    meters_shared: Arc<EngineShared>,
    meters_rx: mpsc::Receiver<MetersPayload>,
    meter_slots: Arc<MixerMeterSlots>,
) {
    std::thread::spawn(move || {
        let mut pending: Vec<MetersPayload> = Vec::with_capacity(8);
        let mut last_emit = Instant::now();
        let mut last_mixer_emit = Instant::now();
        let mut last_logged_drops = 0u64;

        loop {
            let drops = meters_shared.dropped_output_samples.load(Ordering::Relaxed);
            if drops != last_logged_drops {
                log::warn!(
                    "Dropping output samples because the playback ring buffer is full (total dropped: {drops})"
                );
                last_logged_drops = drops;
            }

            match meters_rx.recv_timeout(Duration::from_millis(20)) {
                Ok(meters) => pending.push(meters),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }

            if last_mixer_emit.elapsed() >= Duration::from_millis(MIXER_METERS_INTERVAL_MS) {
                let payload = meter_slots.take_payload();
                // The remote mixer's meter stream reads the latest frame.
                if let Some(latest) = app.try_state::<LatestMeters>() {
                    latest.store(payload.clone());
                }
                let _ = app.emit("mixer-meters", &payload);
                last_mixer_emit = Instant::now();
            }

            if last_emit.elapsed().as_millis() >= 50 && !pending.is_empty() {
                let last = pending.last().cloned().unwrap_or(MetersPayload {
                    input_db: -96.0,
                    output_db: -96.0,
                    gate_gain: 0.0,
                    lufs_m: -70.0,
                    lufs_st: -70.0,
                    lufs_i: -70.0,
                    deess_gr_db: 0.0,
                    auto_gain_db: 0.0,
                    neural_vad: 0.0,
                    spectrum: vec![],
                });

                let spectrum = average_spectrum(&pending).unwrap_or_else(|| last.spectrum.clone());
                let payload = MetersPayload { spectrum, ..last };
                let _ = app.emit("audio-meters", &payload);
                pending.clear();
                last_emit = Instant::now();
            }
        }
        if let Some(latest) = app.try_state::<LatestMeters>() {
            latest.clear();
        }
    });
}

/// Mean of the non-empty spectra in `pending` (None if there are none).
fn average_spectrum(pending: &[MetersPayload]) -> Option<Vec<f32>> {
    let non_empty: Vec<&Vec<f32>> = pending
        .iter()
        .filter(|p| !p.spectrum.is_empty())
        .map(|p| &p.spectrum)
        .collect();
    let first = non_empty.first()?;
    let n = first.len();
    Some(
        (0..n)
            .map(|k| {
                non_empty.iter().map(|s| s.get(k).copied().unwrap_or(0.0)).sum::<f32>()
                    / non_empty.len() as f32
            })
            .collect(),
    )
}


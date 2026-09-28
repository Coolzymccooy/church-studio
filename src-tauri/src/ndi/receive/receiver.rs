//! One receiver thread per selected NDI® source.
//!
//! The thread creates an audio-only NDI receiver, captures audio with a
//! short timeout, converts each frame to interleaved stereo at the engine
//! rate (`convert`) and pushes it into the source's SPSC ring, which the
//! audio callback reads as a mixer strip (`drift`). Every captured frame is
//! freed. The NDI receiver is created, used and destroyed on this thread,
//! so the raw instance pointer never crosses threads.
use super::convert::{planes_to_read, push_stereo_frames, FrameHeader, SourceConverter};
use super::NdiInputStats;
use crate::ndi::ffi::AudioFrameV3;
use crate::ndi::ffi_recv::{empty_audio_frame, RecvInstance, FRAME_TYPE_AUDIO, FRAME_TYPE_ERROR};
use crate::ndi::runtime::NdiRuntime;
use ringbuf::traits::Producer;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Our receiver's name as other NDI tools show it (no "NDI" in it:
/// design decision 3).
pub const RECEIVER_NAME: &str = "TIWATON Studio Input";
/// How long one capture call waits for audio, so `stop` is seen quickly.
const CAPTURE_TIMEOUT_MS: u32 = 50;
/// No audio for this long → the input shows as not connected.
const CONNECTED_WINDOW: Duration = Duration::from_secs(1);
/// Back-off after a capture error (connection lost), before trying again.
const ERROR_BACKOFF: Duration = Duration::from_millis(100);

/// A running receiver. Dropping it stops the thread, joins it and destroys
/// the NDI receiver.
pub struct NdiReceiver {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl NdiReceiver {
    /// Create a receiver for `source` and start pumping its audio into
    /// `producer`. Returns once the receiver exists (or with the reason it
    /// could not be created).
    pub fn start<P>(
        runtime: &'static NdiRuntime,
        source: &str,
        engine_rate: u32,
        producer: P,
        stats: Arc<NdiInputStats>,
    ) -> Result<Self, String>
    where
        P: Producer<Item = f32> + Send + 'static,
    {
        let c_source = CString::new(source)
            .map_err(|_| "the NDI source name contains a NUL character".to_string())?;
        let c_name = CString::new(RECEIVER_NAME)
            .map_err(|_| "the receiver name contains a NUL character".to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<(), String>>(1);

        let thread = std::thread::Builder::new()
            .name("ndi-audio-recv".to_string())
            .spawn(move || {
                let instance = match runtime.recv_create(&c_source, &c_name) {
                    Ok(instance) => instance,
                    Err(err) => {
                        let _ = ready_tx.send(Err(err));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                let mut pump = Pump {
                    converter: SourceConverter::new(engine_rate),
                    producer,
                    stats,
                };
                pump.run(runtime, instance, &thread_stop);
                pump.stats.connected.store(false, Ordering::Relaxed);
                // SAFETY: created above on this thread and no longer used.
                unsafe { runtime.recv_destroy(instance) };
            })
            .map_err(|e| format!("cannot start the NDI receiver thread: {e}"))?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(NdiReceiver {
                stop,
                thread: Some(thread),
            }),
            Ok(Err(err)) => {
                let _ = thread.join();
                Err(err)
            }
            Err(_) => {
                let _ = thread.join();
                Err("the NDI receiver thread exited during start-up".to_string())
            }
        }
    }

    /// Stop the thread and wait for it; the NDI receiver is destroyed on it.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for NdiReceiver {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Pump<P> {
    converter: SourceConverter,
    producer: P,
    stats: Arc<NdiInputStats>,
}

impl<P: Producer<Item = f32>> Pump<P> {
    /// Capture until `stop` is set.
    fn run(&mut self, runtime: &NdiRuntime, instance: RecvInstance, stop: &AtomicBool) {
        let mut last_audio: Option<Instant> = None;
        while !stop.load(Ordering::Acquire) {
            let mut frame = empty_audio_frame();
            // SAFETY: `instance` is live and used only by this thread.
            let kind =
                unsafe { runtime.recv_capture_audio(instance, &mut frame, CAPTURE_TIMEOUT_MS) };
            if kind == FRAME_TYPE_AUDIO {
                self.push_frame(&frame);
                // SAFETY: `frame` was filled by this capture and is freed once.
                unsafe { runtime.recv_free_audio(instance, &frame) };
                last_audio = Some(Instant::now());
            } else if kind == FRAME_TYPE_ERROR {
                // The connection was lost; NDI reconnects by itself.
                last_audio = None;
                std::thread::sleep(ERROR_BACKOFF);
            }
            let connected = last_audio.is_some_and(|at| at.elapsed() < CONNECTED_WINDOW);
            self.stats.connected.store(connected, Ordering::Relaxed);
        }
    }

    /// Convert one captured frame and push it into the ring.
    fn push_frame(&mut self, frame: &AudioFrameV3) {
        let header = FrameHeader {
            sample_rate: frame.sample_rate,
            channels: frame.no_channels,
            samples: frame.no_samples,
            four_cc: frame.four_cc,
            data_addr: frame.p_data as usize,
            stride_bytes: frame.channel_stride_in_bytes,
        };
        let Some(count) = planes_to_read(&header) else {
            self.stats.skipped_frames.fetch_add(1, Ordering::Relaxed);
            return;
        };
        let samples = frame.no_samples as usize;
        let stride = frame.channel_stride_in_bytes as usize;
        let base = frame.p_data as *const u8;
        // SAFETY: `planes_to_read` checked that `p_data` is non-NULL and
        // f32-aligned, that the stride is a whole number of f32 no shorter
        // than one plane, and that `count` ≤ `no_channels`. The SDK
        // guarantees `no_channels` planes of `no_samples` floats, `stride`
        // bytes apart, valid until the frame is freed (after this call).
        let (left, right): (&[f32], &[f32]) = unsafe {
            (
                std::slice::from_raw_parts(base as *const f32, samples),
                std::slice::from_raw_parts(base.add((count - 1) * stride) as *const f32, samples),
            )
        };
        let planes: [&[f32]; 2] = [left, right];
        let stereo = self.converter.convert(frame.sample_rate as u32, &planes[..count]);
        let dropped = push_stereo_frames(&mut self.producer, stereo);
        if dropped > 0 {
            self.stats.dropped_samples.fetch_add(dropped, Ordering::Relaxed);
        }
    }
}

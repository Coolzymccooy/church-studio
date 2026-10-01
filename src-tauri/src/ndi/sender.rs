//! One NDI® audio source per enabled bus, each on its own thread.
//!
//! The audio callback never calls NDI (design decision 4). It pushes the
//! bus's L/R samples, interleaved, into a lock-free SPSC ring buffer (and
//! counts, without logging, what does not fit). The sender thread here pops
//! the ring, frames it into 480-sample FLTP frames and calls
//! `NDIlib_send_send_audio_v3`. The sender is created with `clock_audio`, so
//! each send blocks until it is due: NDI paces the thread in real time.
//!
//! The NDI send instance is created, used and destroyed on the sender
//! thread, so the raw instance pointer never crosses threads.
//!
//! Clock drift: the sound card and NDI's clock differ by a few ppm. Above
//! `DRIFT_HIGH_MS` the pump skips one stereo sample per pass, a correction
//! too small to hear that absorbs far more drift than real clocks have, so
//! latency stays near `DRIFT_HIGH_MS`. Above `HARD_HIGH_MS` (a stall, not
//! drift) the ring is trimmed straight back to `TARGET_FILL_FRAMES`. Both are
//! counted as dropped.
use super::audio_frame::{FltpFramer, NDI_CHANNELS, NDI_FRAME_SAMPLES};
use super::ffi::{AudioFrameV3, SendInstance};
use super::runtime::NdiRuntime;
use ringbuf::traits::{Consumer, Observer};
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::Duration;

/// Ring capacity: one second of stereo audio.
pub const RING_SECONDS: usize = 1;
/// Above this fill one stereo sample is skipped per pump pass (drift).
const DRIFT_HIGH_MS: usize = 60;
/// Above this fill the ring is trimmed back to `TARGET_FILL_FRAMES` frames.
const HARD_HIGH_MS: usize = 500;
const TARGET_FILL_FRAMES: usize = 2;
/// How long the sender sleeps when the ring is empty.
const IDLE_SLEEP: Duration = Duration::from_millis(2);

/// Interleaved-sample capacity of an NDI ring at `sample_rate`.
pub fn ring_capacity(sample_rate: u32) -> usize {
    let one_second = sample_rate as usize * NDI_CHANNELS * RING_SECONDS;
    one_second.max(NDI_FRAME_SAMPLES * NDI_CHANNELS * 8)
}

/// Pops interleaved stereo from the ring and hands out complete FLTP frames.
/// Pure apart from the ring; unit-tested without an NDI runtime.
pub(crate) struct Pump<C> {
    consumer: C,
    framer: FltpFramer,
    scratch: Vec<f32>,
    dropped: Arc<AtomicU64>,
    drift_high: usize,
    hard_high: usize,
    target_fill: usize,
}

impl<C: Consumer<Item = f32>> Pump<C> {
    pub(crate) fn new(sample_rate: u32, consumer: C, dropped: Arc<AtomicU64>) -> Self {
        let frame = NDI_FRAME_SAMPLES * NDI_CHANNELS;
        let ms = |ms: usize| (sample_rate as usize * ms / 1000) * NDI_CHANNELS;
        Pump {
            consumer,
            framer: FltpFramer::new(NDI_CHANNELS, NDI_FRAME_SAMPLES),
            scratch: vec![0.0; frame * 4],
            dropped,
            drift_high: ms(DRIFT_HIGH_MS).max(frame * 4),
            hard_high: ms(HARD_HIGH_MS).max(frame * 8),
            target_fill: frame * TARGET_FILL_FRAMES,
        }
    }

    /// Trim an over-full ring, pop what is available and emit every frame
    /// that completes. Returns the number of samples popped.
    pub(crate) fn pump_once(&mut self, mut emit: impl FnMut(&[f32])) -> usize {
        let occupied = self.consumer.occupied_len();
        // Skip whole sample frames so L stays L.
        let to_skip = if occupied > self.hard_high {
            let excess = occupied - self.target_fill;
            excess - excess % NDI_CHANNELS
        } else if occupied > self.drift_high {
            NDI_CHANNELS
        } else {
            0
        };
        if to_skip > 0 {
            let skipped = self.consumer.skip(to_skip);
            self.dropped.fetch_add(skipped as u64, Ordering::Relaxed);
        }
        let popped = self.consumer.pop_slice(&mut self.scratch[..]);
        if popped > 0 {
            self.framer
                .push_interleaved(&self.scratch[..popped], &mut emit);
        }
        popped
    }
}

/// A running NDI audio source. Dropping it stops the thread, joins it and
/// destroys the NDI sender.
pub struct NdiBusSender {
    name: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl NdiBusSender {
    /// Create the NDI source `name` and start pumping `consumer` into it.
    /// Returns once the source exists (or with the reason it could not be
    /// created).
    pub fn start<C>(
        runtime: &'static NdiRuntime,
        name: String,
        sample_rate: u32,
        consumer: C,
        dropped: Arc<AtomicU64>,
    ) -> Result<Self, String>
    where
        C: Consumer<Item = f32> + Send + 'static,
    {
        let c_name = CString::new(name.clone())
            .map_err(|_| "the NDI source name contains a NUL character".to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<(), String>>(1);

        let thread = std::thread::Builder::new()
            .name("ndi-audio-send".to_string())
            .spawn(move || {
                let instance = match runtime.send_create(&c_name) {
                    Ok(instance) => instance,
                    Err(err) => {
                        let _ = ready_tx.send(Err(err));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                let mut pump = Pump::new(sample_rate, consumer, dropped);
                run(&mut pump, runtime, instance, sample_rate, &thread_stop);
                // SAFETY: created above on this thread and no longer used.
                unsafe { runtime.send_destroy(instance) };
            })
            .map_err(|e| format!("cannot start the NDI sender thread: {e}"))?;

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(NdiBusSender {
                name,
                stop,
                thread: Some(thread),
            }),
            Ok(Err(err)) => {
                let _ = thread.join();
                Err(err)
            }
            Err(_) => {
                let _ = thread.join();
                Err("the NDI sender thread exited during start-up".to_string())
            }
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Stop the thread and wait for it; the NDI sender is destroyed on it.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for NdiBusSender {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Sender-thread loop: pump until `stop` is set. Each `send_audio` blocks
/// until its frame is due (`clock_audio`), so the loop runs in real time.
fn run<C: Consumer<Item = f32>>(
    pump: &mut Pump<C>,
    runtime: &NdiRuntime,
    instance: SendInstance,
    sample_rate: u32,
    stop: &AtomicBool,
) {
    while !stop.load(Ordering::Acquire) {
        let popped = pump.pump_once(|planar| {
            let frame = AudioFrameV3::fltp(sample_rate, NDI_CHANNELS, NDI_FRAME_SAMPLES, planar);
            // SAFETY: `instance` is live and used only by this thread;
            // `planar` holds NDI_CHANNELS × NDI_FRAME_SAMPLES floats and
            // outlives the call.
            unsafe { runtime.send_audio(instance, &frame) };
        });
        if popped == 0 {
            std::thread::sleep(IDLE_SLEEP);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::traits::{Producer, Split};
    use ringbuf::HeapRb;

    #[test]
    fn pump_emits_planar_frames_in_order() {
        let (mut producer, consumer) = HeapRb::<f32>::new(ring_capacity(48_000)).split();
        let dropped = Arc::new(AtomicU64::new(0));
        let mut pump = Pump::new(48_000, consumer, dropped.clone());
        for n in 0..NDI_FRAME_SAMPLES + 10 {
            producer.try_push(n as f32).unwrap();
            producer.try_push(-(n as f32)).unwrap();
        }
        let mut frames: Vec<Vec<f32>> = Vec::new();
        while pump.pump_once(|p| frames.push(p.to_vec())) > 0 {}
        assert_eq!(frames.len(), 1);
        let frame = &frames[0];
        assert_eq!(frame.len(), NDI_FRAME_SAMPLES * NDI_CHANNELS);
        assert_eq!(frame[1], 1.0);
        assert_eq!(frame[NDI_FRAME_SAMPLES + 1], -1.0);
        assert_eq!(frame[NDI_FRAME_SAMPLES - 1], (NDI_FRAME_SAMPLES - 1) as f32);
        assert_eq!(dropped.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn overfull_ring_is_trimmed_and_counted() {
        let (mut producer, consumer) = HeapRb::<f32>::new(ring_capacity(48_000)).split();
        let dropped = Arc::new(AtomicU64::new(0));
        let mut pump = Pump::new(48_000, consumer, dropped.clone());
        // 0.75 s of stereo: above the 0.5 s high-water mark.
        let total = 36_000 * NDI_CHANNELS;
        for n in 0..total {
            producer.try_push(if n % 2 == 0 { 1.0 } else { -1.0 }).unwrap();
        }
        let mut frames = 0usize;
        let mut first_left = None;
        pump.pump_once(|p| {
            frames += 1;
            if first_left.is_none() {
                first_left = Some(p[0]);
            }
        });
        let skipped = dropped.load(Ordering::Relaxed) as usize;
        assert!(skipped > 0);
        assert_eq!(skipped % NDI_CHANNELS, 0, "skips whole sample frames");
        // Channels stay aligned after the skip.
        assert_eq!(first_left, Some(1.0));
        assert!(frames >= 1);
    }

    #[test]
    fn drift_above_soft_mark_skips_one_stereo_sample() {
        let (mut producer, consumer) = HeapRb::<f32>::new(ring_capacity(48_000)).split();
        let dropped = Arc::new(AtomicU64::new(0));
        let mut pump = Pump::new(48_000, consumer, dropped.clone());
        // 0.1 s of stereo: above the 60 ms drift mark, below the hard mark.
        for n in 0..4_800 * NDI_CHANNELS {
            producer.try_push(if n % 2 == 0 { 1.0 } else { -1.0 }).unwrap();
        }
        let mut first_left = None;
        pump.pump_once(|p| {
            if first_left.is_none() {
                first_left = Some(p[0]);
            }
        });
        assert_eq!(dropped.load(Ordering::Relaxed) as usize, NDI_CHANNELS);
        assert_eq!(first_left, Some(1.0));
    }

    #[test]
    fn ring_capacity_is_one_second_of_stereo() {
        assert_eq!(ring_capacity(48_000), 96_000);
        assert!(ring_capacity(8_000) >= NDI_FRAME_SAMPLES * NDI_CHANNELS * 8);
    }
}

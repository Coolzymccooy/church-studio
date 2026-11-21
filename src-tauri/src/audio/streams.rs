//! Building cpal streams around shared feeds, so a stream can be rebuilt on
//! a re-plugged device while the mixer, the rings and every other stream
//! carry on (`docs/specs/2026-11-20-usb-interfaces.md` §3).
//!
//! The callback owns a clone of an `Arc<Mutex<_>>` and only `try_lock`s it:
//! only one stream plays a feed at a time, so the lock is uncontended (one
//! atomic operation) except during the brief rebuild, when the callback
//! plays silence for that block instead of waiting.
use cpal::traits::DeviceTrait;
use cpal::{BufferSize, Device, SampleFormat, StreamConfig, SupportedStreamConfig};
use parking_lot::Mutex;
use ringbuf::HeapCons;
use std::sync::Arc;

use super::health::StreamHealth;
use super::input_proc::InputProcessor;
use super::output_feed::OutputFeed;
use super::{f32_to_i16, f32_to_u16, i16_to_f32, u16_to_f32};

pub(super) type SharedProcessor = Arc<Mutex<InputProcessor<ringbuf::HeapProd<f32>>>>;
pub(super) type SharedFeed = Arc<Mutex<OutputFeed<HeapCons<f32>>>>;

fn stream_config(cfg: &SupportedStreamConfig, buf_frames: u32) -> StreamConfig {
    StreamConfig {
        channels: cfg.channels(),
        sample_rate: cfg.sample_rate(),
        buffer_size: BufferSize::Fixed(buf_frames),
    }
}

fn error_handler(health: Arc<StreamHealth>, label: String) -> impl FnMut(cpal::Error) + Send + 'static {
    move |e| {
        log::error!("{label} stream error: {e}");
        health.on_stream_error(e.kind(), &e.to_string());
    }
}

/// An input stream feeding `processor`.
pub(super) fn build_input(
    device: &Device,
    cfg: &SupportedStreamConfig,
    buf_frames: u32,
    processor: SharedProcessor,
    health: Arc<StreamHealth>,
) -> Result<cpal::Stream, String> {
    let config = stream_config(cfg, buf_frames);
    let errors = error_handler(health, "Input".to_string());
    let stream = match cfg.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            config,
            move |data: &[f32], _| {
                if let Some(mut p) = processor.try_lock() {
                    p.process(data, |s| s);
                }
            },
            errors,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            config,
            move |data: &[i16], _| {
                if let Some(mut p) = processor.try_lock() {
                    p.process(data, i16_to_f32);
                }
            },
            errors,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            config,
            move |data: &[u16], _| {
                if let Some(mut p) = processor.try_lock() {
                    p.process(data, u16_to_f32);
                }
            },
            errors,
            None,
        ),
        fmt => return Err(format!("Unsupported input sample format: {fmt:?}")),
    };
    stream.map_err(|e| e.to_string())
}

/// Silence for a block whose feed is being swapped.
fn silence<T: Copy>(data: &mut [T], zero: T) {
    data.fill(zero);
}

/// An output stream playing `feed`.
pub(super) fn build_output(
    device: &Device,
    cfg: &SupportedStreamConfig,
    buf_frames: u32,
    feed: SharedFeed,
    health: Arc<StreamHealth>,
    label: &str,
) -> Result<cpal::Stream, String> {
    let config = stream_config(cfg, buf_frames);
    let errors = error_handler(health, label.to_string());
    let stream = match cfg.sample_format() {
        SampleFormat::F32 => device.build_output_stream(
            config,
            move |data: &mut [f32], _| match feed.try_lock() {
                Some(mut f) => f.fill(data, |s| s),
                None => silence(data, 0.0),
            },
            errors,
            None,
        ),
        SampleFormat::I16 => device.build_output_stream(
            config,
            move |data: &mut [i16], _| match feed.try_lock() {
                Some(mut f) => f.fill(data, f32_to_i16),
                None => silence(data, 0),
            },
            errors,
            None,
        ),
        SampleFormat::U16 => device.build_output_stream(
            config,
            move |data: &mut [u16], _| match feed.try_lock() {
                Some(mut f) => f.fill(data, f32_to_u16),
                None => silence(data, f32_to_u16(0.0)),
            },
            errors,
            None,
        ),
        fmt => return Err(format!("Unsupported output sample format: {fmt:?}")),
    };
    stream.map_err(|e| e.to_string())
}

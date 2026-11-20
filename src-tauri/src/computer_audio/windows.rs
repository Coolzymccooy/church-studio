//! Windows computer audio: WASAPI loopback of the default output device
//! (cpal opens an input stream on an output device as loopback).
//!
//! Limits, compared with macOS:
//! - Loopback records the whole device, this app included; cpal has no
//!   per-process loopback. So the engine refuses when one of its own
//!   outputs is the default output device, which would feed the mix back
//!   into itself. Play the buses on other devices (or make another device
//!   the Windows default) to use it.
//! - The captured sound keeps playing on the default output device; it is
//!   not muted while it is captured.
use super::capture::CaptureWriter;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use ringbuf::HeapProd;

type Writer = CaptureWriter<HeapProd<f32>>;

pub struct LoopbackCapture {
    _stream: cpal::Stream,
    device_name: String,
    capture_rate: u32,
}

impl LoopbackCapture {
    /// Capture the default output device unless the engine plays to it
    /// (`own_outputs` are the names of the devices the engine opened).
    pub fn start(own_outputs: &[String], make_writer: impl FnOnce(u32) -> Writer) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "Computer audio: Windows has no default output device".to_string())?;
        let device_name = device
            .description()
            .map(|description| description.name().to_string())
            .unwrap_or_else(|_| "Default output".to_string());
        if own_outputs.iter().any(|own| *own == device_name) {
            return Err(format!(
                "Computer audio records the Windows default output, '{device_name}', and this app plays to it too, so the mix would feed back into itself. Choose another device for that bus or make another device the Windows default, then start the engine again. Or turn computer audio off."
            ));
        }
        let config = device
            .default_output_config()
            .map_err(|e| format!("Computer audio: cannot read '{device_name}': {e}"))?;
        let capture_rate = config.sample_rate();
        let channels = (config.channels() as usize).max(1);
        let stream_config = StreamConfig {
            channels: config.channels(),
            sample_rate: config.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };
        let mut writer = make_writer(capture_rate);
        let on_error = |e| log::error!("computer audio loopback error: {e}");
        let stream = match config.sample_format() {
            SampleFormat::F32 => device.build_input_stream(
                stream_config,
                move |data: &[f32], _| writer.write_interleaved(data, channels),
                on_error,
                None,
            ),
            fmt => {
                return Err(format!(
                    "Computer audio: '{device_name}' uses {fmt:?} samples, which loopback cannot read"
                ))
            }
        }
        .map_err(|e| format!("Computer audio: cannot record '{device_name}': {e}"))?;
        stream
            .play()
            .map_err(|e| format!("Computer audio: cannot start recording '{device_name}': {e}"))?;
        Ok(LoopbackCapture {
            _stream: stream,
            device_name,
            capture_rate,
        })
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub fn capture_rate(&self) -> u32 {
        self.capture_rate
    }
}

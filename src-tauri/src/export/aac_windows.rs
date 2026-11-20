//! AAC in an MPEG-4 audio file (.m4a) through Media Foundation's sink
//! writer and the Microsoft AAC encoder that ships with Windows, so nothing
//! is bundled.
//!
//! The Microsoft encoder takes 16-bit PCM at 44.1 or 48 kHz, mono or stereo,
//! and offers 96, 128, 160 or 192 kbps. Samples are converted to 16-bit
//! here; other sample rates are refused with a hint to export MP3.
//!
//! NOT YET RUN ON WINDOWS: type-checked for x86_64-pc-windows-msvc on the
//! Mac only (see the PR).
use super::{clamp_into, AudioSink, EncodeSpec};
use std::path::Path;
use windows::core::{GUID, HSTRING, PCWSTR};
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFMediaType, IMFSinkWriter, MFAudioFormat_AAC, MFAudioFormat_PCM, MFCreateAttributes,
    MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFCreateSinkWriterFromURL, MFMediaType_Audio,
    MFShutdown, MFStartup, MFTranscodeContainerType_MPEG4, MFSTARTUP_NOSOCKET, MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
    MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS,
    MF_MT_AUDIO_SAMPLES_PER_SECOND, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_TRANSCODE_CONTAINERTYPE, MF_VERSION,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

/// Media Foundation time unit: 100 ns.
const TICKS_PER_SECOND: i64 = 10_000_000;

/// The Microsoft AAC encoder's bitrates, as bytes per second.
pub fn aac_bytes_per_second(kbps: u32) -> u32 {
    match kbps {
        0..=96 => 12_000,
        97..=128 => 16_000,
        129..=160 => 20_000,
        _ => 24_000,
    }
}

pub fn check_rate(sample_rate: u32) -> Result<(), String> {
    if sample_rate == 44_100 || sample_rate == 48_000 {
        Ok(())
    } else {
        Err(format!(
            "m4a on Windows needs a 44.1 or 48 kHz recording (this one is {} Hz); export mp3 instead",
            sample_rate
        ))
    }
}

/// f32 → little-endian i16 bytes (input already clamped to ±1).
pub fn to_pcm16(samples: &[f32], out: &mut Vec<u8>) {
    out.clear();
    out.reserve(samples.len() * 2);
    for s in samples {
        let v = (s * 32_767.0).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
}

pub struct AacSink {
    writer: Option<IMFSinkWriter>,
    stream: u32,
    channels: u32,
    sample_rate: u32,
    frames_written: i64,
    clamped: Vec<f32>,
    pcm: Vec<u8>,
    com_initialised: bool,
    mf_started: bool,
}

fn mf_err(what: &str) -> impl Fn(windows::core::Error) -> String + '_ {
    move |e| format!("AAC encoder: {what} failed ({e})")
}

fn audio_type(subtype: &GUID, spec: &EncodeSpec) -> Result<IMFMediaType, String> {
    let channels = spec.channels as u32;
    // SAFETY: plain COM calls on a media type this function owns.
    unsafe {
        let media = MFCreateMediaType().map_err(mf_err("media type"))?;
        media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(mf_err("major type"))?;
        media.SetGUID(&MF_MT_SUBTYPE, subtype).map_err(mf_err("subtype"))?;
        media.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).map_err(mf_err("bits"))?;
        media
            .SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, spec.sample_rate)
            .map_err(mf_err("sample rate"))?;
        media.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels).map_err(mf_err("channels"))?;
        Ok(media)
    }
}

impl AacSink {
    pub fn create(path: &Path, spec: &EncodeSpec) -> Result<Self, String> {
        check_rate(spec.sample_rate)?;
        let mut sink = AacSink {
            writer: None,
            stream: 0,
            channels: spec.channels as u32,
            sample_rate: spec.sample_rate,
            frames_written: 0,
            clamped: Vec::new(),
            pcm: Vec::new(),
            com_initialised: false,
            mf_started: false,
        };
        // SAFETY: COM and Media Foundation start-up, balanced in Drop.
        unsafe {
            // S_OK or S_FALSE need a matching CoUninitialize; a thread already
            // in another apartment (RPC_E_CHANGED_MODE) can still use MF.
            sink.com_initialised = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET).map_err(mf_err("start-up"))?;
            sink.mf_started = true;

            let mut attributes: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attributes, 1).map_err(mf_err("attributes"))?;
            let attributes = attributes.ok_or("AAC encoder: no attributes")?;
            // The output is `<name>.m4a.part`, so the container is set
            // explicitly instead of being guessed from the extension.
            attributes
                .SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4)
                .map_err(mf_err("container"))?;
            let url = HSTRING::from(path.as_os_str());
            let writer = MFCreateSinkWriterFromURL(PCWSTR(url.as_ptr()), None, &attributes)
                .map_err(mf_err("create"))?;

            let output = audio_type(&MFAudioFormat_AAC, spec)?;
            output
                .SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, aac_bytes_per_second(spec.bitrate_kbps))
                .map_err(mf_err("bitrate"))?;
            let stream = writer.AddStream(&output).map_err(mf_err("output format"))?;

            let input = audio_type(&MFAudioFormat_PCM, spec)?;
            let block = sink.channels * 2;
            input.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, block).map_err(mf_err("block align"))?;
            input
                .SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, block * spec.sample_rate)
                .map_err(mf_err("byte rate"))?;
            writer.SetInputMediaType(stream, &input, None).map_err(mf_err("input format"))?;
            writer.BeginWriting().map_err(mf_err("begin"))?;
            sink.stream = stream;
            sink.writer = Some(writer);
        }
        Ok(sink)
    }
}

impl AudioSink for AacSink {
    fn write(&mut self, interleaved: &[f32]) -> Result<(), String> {
        let writer = self.writer.as_ref().ok_or("AAC encoder: not open")?;
        clamp_into(interleaved, &mut self.clamped);
        to_pcm16(&self.clamped, &mut self.pcm);
        let frames = (self.clamped.len() / self.channels as usize) as i64;
        if frames == 0 {
            return Ok(());
        }
        let rate = self.sample_rate as i64;
        // SAFETY: the buffer is locked for exactly `pcm.len()` bytes while
        // copying, then unlocked before it is handed to the writer.
        unsafe {
            let buffer = MFCreateMemoryBuffer(self.pcm.len() as u32).map_err(mf_err("buffer"))?;
            let mut data: *mut u8 = std::ptr::null_mut();
            buffer.Lock(&mut data, None, None).map_err(mf_err("lock"))?;
            std::ptr::copy_nonoverlapping(self.pcm.as_ptr(), data, self.pcm.len());
            buffer.Unlock().map_err(mf_err("unlock"))?;
            buffer.SetCurrentLength(self.pcm.len() as u32).map_err(mf_err("length"))?;
            let sample = MFCreateSample().map_err(mf_err("sample"))?;
            sample.AddBuffer(&buffer).map_err(mf_err("add buffer"))?;
            let start = self.frames_written * TICKS_PER_SECOND / rate;
            let end = (self.frames_written + frames) * TICKS_PER_SECOND / rate;
            sample.SetSampleTime(start).map_err(mf_err("time"))?;
            sample.SetSampleDuration(end - start).map_err(mf_err("duration"))?;
            writer.WriteSample(self.stream, &sample).map_err(mf_err("write"))?;
        }
        self.frames_written += frames;
        Ok(())
    }

    fn finish(mut self: Box<Self>) -> Result<(), String> {
        let writer = self.writer.take().ok_or("AAC encoder: not open")?;
        // SAFETY: Finalize writes the last packets and the MPEG-4 index.
        unsafe { writer.Finalize() }.map_err(mf_err("finishing the file"))
    }
}

impl Drop for AacSink {
    fn drop(&mut self) {
        // The writer is released before Media Foundation shuts down.
        self.writer = None;
        // SAFETY: balanced with the start-up calls in `create`.
        unsafe {
            if self.mf_started {
                let _ = MFShutdown();
            }
            if self.com_initialised {
                CoUninitialize();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitrates_map_to_the_encoders_steps() {
        assert_eq!(aac_bytes_per_second(96), 12_000);
        assert_eq!(aac_bytes_per_second(128), 16_000);
        assert_eq!(aac_bytes_per_second(192), 24_000);
        assert_eq!(aac_bytes_per_second(320), 24_000);
    }

    #[test]
    fn only_44_1_and_48_khz_are_accepted() {
        assert!(check_rate(48_000).is_ok());
        assert!(check_rate(44_100).is_ok());
        assert!(check_rate(96_000).unwrap_err().contains("mp3"));
    }

    #[test]
    fn pcm16_conversion_is_little_endian_and_full_scale() {
        let mut out = Vec::new();
        to_pcm16(&[1.0, -1.0, 0.0], &mut out);
        assert_eq!(out, vec![0xFF, 0x7F, 0x01, 0x80, 0x00, 0x00]);
    }
}

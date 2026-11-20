//! AAC in an MPEG-4 audio file (.m4a) through AudioToolbox's ExtAudioFile:
//! Apple's own encoder, part of macOS, so nothing is bundled and there is no
//! licence to carry. The file's sample rate is capped at 48 kHz (AAC-LC);
//! ExtAudioFile resamples a faster recording on the way in.
use super::{clamp_into, AudioSink, EncodeSpec};
use objc2_audio_toolbox::{
    kAudioConverterEncodeBitRate, kAudioFileM4AType, kExtAudioFileProperty_AudioConverter,
    kExtAudioFileProperty_ClientDataFormat, kExtAudioFileProperty_ConverterConfig,
    AudioConverterRef, AudioConverterSetProperty, AudioFileFlags, ExtAudioFileCreateWithURL,
    ExtAudioFileDispose, ExtAudioFileGetProperty, ExtAudioFileRef, ExtAudioFileSetProperty,
    ExtAudioFileWrite,
};
use objc2_core_audio_types::{
    kAudioFormatFlagIsFloat, kAudioFormatFlagIsPacked, kAudioFormatLinearPCM, kAudioFormatMPEG4AAC,
    AudioBuffer, AudioBufferList, AudioStreamBasicDescription,
};
use objc2_core_foundation::CFURL;
use std::ffi::c_void;
use std::path::Path;
use std::ptr::{self, NonNull};

/// Highest sample rate written to the file.
pub const MAX_FILE_RATE: u32 = 48_000;

pub struct AacSink {
    file: ExtAudioFileRef,
    channels: u32,
    clamped: Vec<f32>,
}

/// "OSStatus 1718449215 ('fmt?')" style text.
fn status_text(status: i32) -> String {
    let bytes = status.to_be_bytes();
    if bytes.iter().all(|b| b.is_ascii_graphic() || *b == b' ') {
        format!("{status} ('{}')", String::from_utf8_lossy(&bytes))
    } else {
        status.to_string()
    }
}

fn check(status: i32, what: &str) -> Result<(), String> {
    if status == 0 {
        Ok(())
    } else {
        Err(format!("AAC encoder: {what} failed ({})", status_text(status)))
    }
}

fn file_format(spec: &EncodeSpec) -> AudioStreamBasicDescription {
    AudioStreamBasicDescription {
        mSampleRate: spec.sample_rate.min(MAX_FILE_RATE) as f64,
        mFormatID: kAudioFormatMPEG4AAC,
        mFormatFlags: 0,
        mBytesPerPacket: 0,
        mFramesPerPacket: 1024,
        mBytesPerFrame: 0,
        mChannelsPerFrame: spec.channels as u32,
        mBitsPerChannel: 0,
        mReserved: 0,
    }
}

fn client_format(spec: &EncodeSpec) -> AudioStreamBasicDescription {
    let bytes = 4 * spec.channels as u32;
    AudioStreamBasicDescription {
        mSampleRate: spec.sample_rate as f64,
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
        mBytesPerPacket: bytes,
        mFramesPerPacket: 1,
        mBytesPerFrame: bytes,
        mChannelsPerFrame: spec.channels as u32,
        mBitsPerChannel: 32,
        mReserved: 0,
    }
}

impl AacSink {
    pub fn create(path: &Path, spec: &EncodeSpec) -> Result<Self, String> {
        let url = CFURL::from_file_path(path)
            .ok_or_else(|| format!("cannot use {} as a file path", path.display()))?;
        let file_desc = file_format(spec);
        let mut file: ExtAudioFileRef = ptr::null_mut();
        // SAFETY: every pointer refers to a live local; the layout is null.
        let status = unsafe {
            ExtAudioFileCreateWithURL(
                &url,
                kAudioFileM4AType,
                NonNull::from(&file_desc),
                ptr::null(),
                AudioFileFlags::EraseFile.0,
                NonNull::from(&mut file),
            )
        };
        check(status, "create")?;
        let sink = AacSink {
            file,
            channels: spec.channels as u32,
            clamped: Vec::new(),
        };
        let client = client_format(spec);
        // SAFETY: `sink.file` is open; the property is a whole ASBD.
        let status = unsafe {
            ExtAudioFileSetProperty(
                sink.file,
                kExtAudioFileProperty_ClientDataFormat,
                std::mem::size_of::<AudioStreamBasicDescription>() as u32,
                NonNull::from(&client).cast(),
            )
        };
        check(status, "input format")?;
        if let Err(err) = sink.set_bitrate(spec.bitrate_kbps) {
            // The encoder's own default rate is still a good file.
            log::warn!("export: {err}; keeping the AAC encoder's default bitrate");
        }
        Ok(sink)
    }

    fn set_bitrate(&self, kbps: u32) -> Result<(), String> {
        let mut converter: AudioConverterRef = ptr::null_mut();
        let mut size = std::mem::size_of::<AudioConverterRef>() as u32;
        // SAFETY: `converter` and `size` are live locals of the sizes given.
        let status = unsafe {
            ExtAudioFileGetProperty(
                self.file,
                kExtAudioFileProperty_AudioConverter,
                NonNull::from(&mut size),
                NonNull::from(&mut converter).cast(),
            )
        };
        check(status, "reading the converter")?;
        if converter.is_null() {
            return Err("AAC encoder: no converter".to_string());
        }
        let bits_per_second: u32 = kbps * 1000;
        // SAFETY: the converter belongs to the open file; the value is a u32.
        let status = unsafe {
            AudioConverterSetProperty(
                converter,
                kAudioConverterEncodeBitRate,
                4,
                NonNull::from(&bits_per_second).cast(),
            )
        };
        check(status, &format!("{kbps} kbps"))?;
        // A null config tells ExtAudioFile to pick up the converter change.
        let config: *const c_void = ptr::null();
        // SAFETY: the property is one pointer-sized value.
        let status = unsafe {
            ExtAudioFileSetProperty(
                self.file,
                kExtAudioFileProperty_ConverterConfig,
                std::mem::size_of::<*const c_void>() as u32,
                NonNull::from(&config).cast(),
            )
        };
        check(status, "applying the bitrate")
    }

    fn dispose(&mut self) -> i32 {
        if self.file.is_null() {
            return 0;
        }
        // SAFETY: disposed once; the field is cleared right after.
        let status = unsafe { ExtAudioFileDispose(self.file) };
        self.file = ptr::null_mut();
        status
    }
}

impl AudioSink for AacSink {
    fn write(&mut self, interleaved: &[f32]) -> Result<(), String> {
        clamp_into(interleaved, &mut self.clamped);
        let frames = self.clamped.len() / self.channels as usize;
        if frames == 0 {
            return Ok(());
        }
        let mut list = AudioBufferList {
            mNumberBuffers: 1,
            mBuffers: [AudioBuffer {
                mNumberChannels: self.channels,
                mDataByteSize: (frames * self.channels as usize * 4) as u32,
                mData: self.clamped.as_mut_ptr().cast(),
            }],
        };
        // SAFETY: the list points at `self.clamped`, alive for the call.
        let status = unsafe { ExtAudioFileWrite(self.file, frames as u32, NonNull::from(&mut list)) };
        check(status, "write")
    }

    fn finish(mut self: Box<Self>) -> Result<(), String> {
        // Dispose writes the last packets and the MPEG-4 index.
        let status = self.dispose();
        check(status, "finishing the file")
    }
}

impl Drop for AacSink {
    fn drop(&mut self) {
        self.dispose();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_codes_read_as_four_char_codes() {
        assert_eq!(status_text(0x666d743f), "1718449215 ('fmt?')");
        assert_eq!(status_text(-50), "-50");
    }

    #[test]
    fn the_file_rate_is_capped_for_aac() {
        let spec = EncodeSpec { channels: 2, sample_rate: 96_000, bitrate_kbps: 192, title: None };
        assert_eq!(file_format(&spec).mSampleRate, 48_000.0);
        assert_eq!(client_format(&spec).mSampleRate, 96_000.0);
        assert_eq!(client_format(&spec).mBytesPerFrame, 8);
    }
}

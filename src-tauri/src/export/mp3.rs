//! MP3 through LAME 3.100 (`mp3lame-encoder` 0.2): constant bitrate, LAME
//! quality 2 (its recommended high-quality setting), joint stereo for stereo
//! input. No Xing/LAME info frame: a CBR file needs none to report its
//! length, and the wrapper cannot rewrite it after the fact.
use super::{AudioSink, EncodeSpec};
use mp3lame_encoder::{Bitrate, Builder, Encoder, FlushGap, Id3Tag, InterleavedPcm, Mode, MonoPcm, Quality};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// The CBR rates offered (kbps).
pub const BITRATES: [u32; 6] = [96, 128, 160, 192, 256, 320];

fn bitrate(kbps: u32) -> Result<Bitrate, String> {
    Ok(match kbps {
        96 => Bitrate::Kbps96,
        128 => Bitrate::Kbps128,
        160 => Bitrate::Kbps160,
        192 => Bitrate::Kbps192,
        256 => Bitrate::Kbps256,
        320 => Bitrate::Kbps320,
        other => return Err(format!("unsupported MP3 bitrate {other} kbps")),
    })
}

pub struct Mp3Sink {
    encoder: Encoder,
    channels: usize,
    file: BufWriter<File>,
    /// Reused encode output and clamped-input buffers.
    out: Vec<u8>,
    clamped: Vec<f32>,
}

impl Mp3Sink {
    pub fn create(path: &Path, spec: &EncodeSpec) -> Result<Self, String> {
        let mut builder = Builder::new().ok_or("MP3 encoder: out of memory")?;
        let fail = |what: &str, e: &dyn std::fmt::Display| format!("MP3 encoder: {what}: {e}");
        builder
            .set_num_channels(spec.channels as u8)
            .map_err(|e| fail("channels", &e))?;
        builder
            .set_sample_rate(spec.sample_rate)
            .map_err(|e| fail("sample rate", &e))?;
        builder
            .set_brate(bitrate(spec.bitrate_kbps)?)
            .map_err(|e| fail("bitrate", &e))?;
        let mode = if spec.channels == 1 { Mode::Mono } else { Mode::JointStereo };
        builder.set_mode(mode).map_err(|e| fail("mode", &e))?;
        builder
            .set_quality(Quality::NearBest)
            .map_err(|e| fail("quality", &e))?;
        builder
            .set_to_write_vbr_tag(false)
            .map_err(|e| fail("info tag", &e))?;
        if let Some(title) = spec.title.as_deref().filter(|t| !t.is_empty()) {
            builder
                .set_id3_tag(Id3Tag {
                    title: title.as_bytes(),
                    artist: b"",
                    album: b"",
                    album_art: b"",
                    year: b"",
                    comment: b"TIWATON AI Studio",
                })
                .map_err(|e| format!("MP3 encoder: title tag: {e:?}"))?;
        }
        let encoder = builder.build().map_err(|e| fail("setup", &e))?;
        let file = File::create(path).map_err(|e| format!("cannot create {}: {e}", path.display()))?;
        Ok(Mp3Sink {
            encoder,
            channels: spec.channels as usize,
            file: BufWriter::with_capacity(256 * 1024, file),
            out: Vec::new(),
            clamped: Vec::new(),
        })
    }
}

/// LAME's float input is ±1 full scale; the recorder keeps overs.
pub fn clamp_into(samples: &[f32], out: &mut Vec<f32>) {
    out.clear();
    out.extend(samples.iter().map(|s| if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 }));
}

impl AudioSink for Mp3Sink {
    fn write(&mut self, interleaved: &[f32]) -> Result<(), String> {
        clamp_into(interleaved, &mut self.clamped);
        let frames = self.clamped.len() / self.channels;
        self.out.clear();
        self.out.reserve(mp3lame_encoder::max_required_buffer_size(frames));
        let result = if self.channels == 1 {
            self.encoder.encode_to_vec(MonoPcm(&self.clamped[..]), &mut self.out)
        } else {
            self.encoder.encode_to_vec(InterleavedPcm(&self.clamped[..]), &mut self.out)
        };
        result.map_err(|e| format!("MP3 encoder: {e}"))?;
        self.file
            .write_all(&self.out)
            .map_err(|e| format!("cannot write the MP3: {e}"))
    }

    fn finish(mut self: Box<Self>) -> Result<(), String> {
        self.out.clear();
        self.out.reserve(7200);
        self.encoder
            .flush_to_vec::<FlushGap>(&mut self.out)
            .map_err(|e| format!("MP3 encoder: {e}"))?;
        self.file
            .write_all(&self.out)
            .and_then(|_| self.file.flush())
            .and_then(|_| self.file.get_ref().sync_all())
            .map_err(|e| format!("cannot write the MP3: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_offered_bitrates_are_accepted() {
        for kbps in BITRATES {
            assert!(bitrate(kbps).is_ok(), "{kbps}");
        }
        assert!(bitrate(100).is_err());
    }

    #[test]
    fn overs_and_bad_samples_are_clamped() {
        let mut out = Vec::new();
        clamp_into(&[1.5, -2.0, 0.25, f32::NAN, f32::INFINITY], &mut out);
        assert_eq!(out, vec![1.0, -1.0, 0.25, 0.0, 0.0]);
    }
}

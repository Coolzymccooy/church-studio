//! A small hand-written RIFF/WAVE writer for 32-bit float tracks (design
//! decisions 2 and 3).
//!
//! Layout (58-byte header, then interleaved little-endian f32 samples):
//! `RIFF <size> WAVE`, `fmt ` (18 bytes, `WAVE_FORMAT_IEEE_FLOAT`,
//! cbSize 0), `fact` (frame count, required for non-PCM formats) and `data`.
//!
//! The header is written with zero sizes at create and rewritten by
//! `patch_header` (every few seconds and at finish), so a power cut leaves a
//! file that plays up to the last patch. A part rolls over to
//! `<base> part N.wav` before its data would pass `max_data_bytes`
//! (classic WAV sizes are 32-bit).
use super::names::part_file_name;
use std::fs::File;
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// `WAVE_FORMAT_IEEE_FLOAT`.
pub const FORMAT_IEEE_FLOAT: u16 = 3;
pub const BYTES_PER_SAMPLE: u16 = 4;
pub const HEADER_LEN: usize = 58;
/// Roll over before 4 GiB: ~3.9 GB of data per part (~5.6 h mono at 48 kHz).
pub const MAX_DATA_BYTES: u64 = 3_900_000_000;

/// Byte offsets of the size fields patched in place.
const RIFF_SIZE_AT: u64 = 4;
const FACT_FRAMES_AT: u64 = 46;
const DATA_SIZE_AT: u64 = 54;

/// The 58-byte header for `channels` float channels holding `data_bytes`
/// of samples.
pub fn header_bytes(channels: u16, sample_rate: u32, data_bytes: u32) -> [u8; HEADER_LEN] {
    let channels = channels.max(1);
    let block_align = channels * BYTES_PER_SAMPLE;
    let byte_rate = sample_rate * block_align as u32;
    let frames = data_bytes / block_align as u32;
    let mut h = [0u8; HEADER_LEN];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&riff_size(data_bytes).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&18u32.to_le_bytes());
    h[20..22].copy_from_slice(&FORMAT_IEEE_FLOAT.to_le_bytes());
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    h[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    h[32..34].copy_from_slice(&block_align.to_le_bytes());
    h[34..36].copy_from_slice(&(BYTES_PER_SAMPLE * 8).to_le_bytes());
    // h[36..38]: cbSize = 0.
    h[38..42].copy_from_slice(b"fact");
    h[42..46].copy_from_slice(&4u32.to_le_bytes());
    h[46..50].copy_from_slice(&frames.to_le_bytes());
    h[50..54].copy_from_slice(b"data");
    h[54..58].copy_from_slice(&data_bytes.to_le_bytes());
    h
}

/// RIFF chunk size: the whole file minus the 8-byte RIFF chunk header.
pub fn riff_size(data_bytes: u32) -> u32 {
    (HEADER_LEN as u32 - 8).saturating_add(data_bytes)
}

/// Whole frames of `channels` that still fit in a part holding
/// `data_bytes` of at most `max_data_bytes`.
pub fn frames_that_fit(data_bytes: u64, max_data_bytes: u64, channels: usize) -> usize {
    let block = (channels.max(1) * BYTES_PER_SAMPLE as usize) as u64;
    (max_data_bytes.saturating_sub(data_bytes) / block) as usize
}

/// One track (mono strip or stereo bus), possibly split over several parts.
pub struct TrackWriter {
    dir: PathBuf,
    base: String,
    channels: usize,
    sample_rate: u32,
    max_data_bytes: u64,
    part: u32,
    file: BufWriter<File>,
    data_bytes: u64,
    /// File names of every part, in order.
    files: Vec<String>,
}

impl TrackWriter {
    pub fn create(dir: &Path, base: &str, channels: usize, sample_rate: u32) -> io::Result<Self> {
        Self::create_with_limit(dir, base, channels, sample_rate, MAX_DATA_BYTES)
    }

    /// `create` with a custom part size (tests use a tiny one).
    pub fn create_with_limit(
        dir: &Path,
        base: &str,
        channels: usize,
        sample_rate: u32,
        max_data_bytes: u64,
    ) -> io::Result<Self> {
        let channels = channels.max(1);
        let name = part_file_name(base, 1);
        let file = open_part(&dir.join(&name), channels, sample_rate)?;
        Ok(TrackWriter {
            dir: dir.to_path_buf(),
            base: base.to_string(),
            channels,
            sample_rate,
            max_data_bytes: max_data_bytes.min(u32::MAX as u64),
            part: 1,
            file,
            data_bytes: 0,
            files: vec![name],
        })
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn files(&self) -> &[String] {
        &self.files
    }

    /// Append interleaved samples (whole frames of `channels`), rolling over
    /// to a new part whenever the current one is full.
    pub fn write_samples(&mut self, samples: &[f32]) -> io::Result<()> {
        let whole = samples.len() - samples.len() % self.channels;
        let mut rest = &samples[..whole];
        while !rest.is_empty() {
            let mut fit = frames_that_fit(self.data_bytes, self.max_data_bytes, self.channels);
            if fit == 0 {
                self.roll_over()?;
                fit = frames_that_fit(self.data_bytes, self.max_data_bytes, self.channels).max(1);
            }
            let take = (fit * self.channels).min(rest.len());
            for sample in &rest[..take] {
                self.file.write_all(&sample.to_le_bytes())?;
            }
            self.data_bytes += (take * BYTES_PER_SAMPLE as usize) as u64;
            rest = &rest[take..];
        }
        Ok(())
    }

    /// Flush and rewrite the size fields of the current part, then return
    /// to the end so writing can continue.
    pub fn patch_header(&mut self) -> io::Result<()> {
        self.file.flush()?;
        let data = self.data_bytes.min(u32::MAX as u64) as u32;
        let block = (self.channels * BYTES_PER_SAMPLE as usize) as u32;
        let file = self.file.get_mut();
        file.seek(SeekFrom::Start(RIFF_SIZE_AT))?;
        file.write_all(&riff_size(data).to_le_bytes())?;
        file.seek(SeekFrom::Start(FACT_FRAMES_AT))?;
        file.write_all(&(data / block).to_le_bytes())?;
        file.seek(SeekFrom::Start(DATA_SIZE_AT))?;
        file.write_all(&data.to_le_bytes())?;
        file.seek(SeekFrom::End(0))?;
        Ok(())
    }

    /// Patch the header, sync to disk and return the part file names.
    pub fn finish(mut self) -> io::Result<Vec<String>> {
        self.patch_header()?;
        self.file.get_ref().sync_all()?;
        Ok(self.files)
    }

    fn roll_over(&mut self) -> io::Result<()> {
        self.patch_header()?;
        self.part += 1;
        let name = part_file_name(&self.base, self.part);
        self.file = open_part(&self.dir.join(&name), self.channels, self.sample_rate)?;
        self.data_bytes = 0;
        self.files.push(name);
        Ok(())
    }
}

fn open_part(path: &Path, channels: usize, sample_rate: u32) -> io::Result<BufWriter<File>> {
    let mut file = BufWriter::with_capacity(256 * 1024, File::create(path)?);
    file.write_all(&header_bytes(channels as u16, sample_rate, 0))?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16_at(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([bytes[at], bytes[at + 1]])
    }

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    }

    #[test]
    fn mono_float_header_reads_back() {
        let h = header_bytes(1, 48_000, 0);
        assert_eq!(&h[0..4], b"RIFF");
        assert_eq!(u32_at(&h, 4), 50);
        assert_eq!(&h[8..16], b"WAVEfmt ");
        assert_eq!(u32_at(&h, 16), 18);
        assert_eq!(u16_at(&h, 20), FORMAT_IEEE_FLOAT);
        assert_eq!(u16_at(&h, 22), 1);
        assert_eq!(u32_at(&h, 24), 48_000);
        assert_eq!(u32_at(&h, 28), 192_000);
        assert_eq!(u16_at(&h, 32), 4);
        assert_eq!(u16_at(&h, 34), 32);
        assert_eq!(u16_at(&h, 36), 0);
        assert_eq!(&h[38..42], b"fact");
        assert_eq!(&h[50..54], b"data");
        assert_eq!(u32_at(&h, 54), 0);
    }

    #[test]
    fn stereo_header_has_sizes_for_the_data() {
        let h = header_bytes(2, 44_100, 800);
        assert_eq!(u16_at(&h, 22), 2);
        assert_eq!(u32_at(&h, 28), 44_100 * 8);
        assert_eq!(u16_at(&h, 32), 8);
        assert_eq!(u32_at(&h, 4), 50 + 800);
        assert_eq!(u32_at(&h, 46), 100, "fact holds frames");
        assert_eq!(u32_at(&h, 54), 800);
    }

    #[test]
    fn frames_that_fit_counts_whole_frames_only() {
        assert_eq!(frames_that_fit(0, 16, 1), 4);
        assert_eq!(frames_that_fit(0, 20, 2), 2);
        assert_eq!(frames_that_fit(16, 16, 1), 0);
        assert_eq!(frames_that_fit(20, 16, 1), 0);
        // 3.9 GB mono at 48 kHz is about 5.6 hours.
        let hours = frames_that_fit(0, MAX_DATA_BYTES, 1) as f64 / 48_000.0 / 3600.0;
        assert!((5.5..5.7).contains(&hours), "{hours}");
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tiwaton-wav-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn patched_file_reads_back_sizes_and_samples() {
        let dir = temp_dir("patch");
        let mut track = TrackWriter::create(&dir, "01 Pastor", 1, 48_000).unwrap();
        track.write_samples(&[0.25, -0.5, 1.5]).unwrap();
        let files = track.finish().unwrap();
        assert_eq!(files, vec!["01 Pastor.wav".to_string()]);
        let bytes = std::fs::read(dir.join("01 Pastor.wav")).unwrap();
        assert_eq!(bytes.len(), HEADER_LEN + 12);
        assert_eq!(u32_at(&bytes, 4), 50 + 12);
        assert_eq!(u32_at(&bytes, 46), 3);
        assert_eq!(u32_at(&bytes, 54), 12);
        let third = f32::from_le_bytes([bytes[66], bytes[67], bytes[68], bytes[69]]);
        assert_eq!(third, 1.5, "float keeps values above full scale");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_full_part_rolls_over_on_a_frame_boundary() {
        let dir = temp_dir("roll");
        // 3 stereo frames (24 bytes) per part.
        let mut track = TrackWriter::create_with_limit(&dir, "Stream Mix", 2, 48_000, 28).unwrap();
        let samples: Vec<f32> = (0..10).map(|i| i as f32).collect();
        track.write_samples(&samples).unwrap();
        let files = track.finish().unwrap();
        assert_eq!(files, vec!["Stream Mix.wav".to_string(), "Stream Mix part 2.wav".to_string()]);
        let first = std::fs::read(dir.join("Stream Mix.wav")).unwrap();
        let second = std::fs::read(dir.join("Stream Mix part 2.wav")).unwrap();
        assert_eq!(u32_at(&first, 54), 24);
        assert_eq!(u32_at(&second, 54), 16);
        assert_eq!(u32_at(&second, 46), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

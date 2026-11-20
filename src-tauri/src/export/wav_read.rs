//! A streaming WAV reader for export: the recorder's 32-bit float tracks,
//! and the 16/24/32-bit PCM or float WAVs the editor hands over.
//!
//! Reads `fmt ` and `data`, skipping every other chunk. A `data` size of 0
//! or one that runs past the end of the file (a recording cut off before its
//! last header patch) is read to the end of the file instead.
use std::io::{self, Read, Seek, SeekFrom};

const FORMAT_PCM: u16 = 1;
const FORMAT_IEEE_FLOAT: u16 = 3;
const FORMAT_EXTENSIBLE: u16 = 0xFFFE;
/// Most channels an export accepts (mono strips, stereo buses).
pub const MAX_CHANNELS: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    Int16,
    Int24,
    Int32,
    Float32,
}

impl SampleFormat {
    fn bytes(self) -> usize {
        match self {
            SampleFormat::Int16 => 2,
            SampleFormat::Int24 => 3,
            SampleFormat::Int32 | SampleFormat::Float32 => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavFormat {
    pub channels: u16,
    pub sample_rate: u32,
    pub sample: SampleFormat,
}

impl WavFormat {
    pub fn block_align(&self) -> usize {
        self.channels as usize * self.sample.bytes()
    }
}

pub struct WavReader<R> {
    inner: R,
    format: WavFormat,
    /// Whole frames left to read.
    frames_left: u64,
    total_frames: u64,
    bytes: Vec<u8>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Parse a `fmt ` chunk body.
pub fn parse_fmt(body: &[u8]) -> io::Result<WavFormat> {
    if body.len() < 16 {
        return Err(invalid("WAV fmt chunk is too short"));
    }
    let mut tag = u16_at(body, 0);
    let channels = u16_at(body, 2);
    let sample_rate = u32_at(body, 4);
    let bits = u16_at(body, 14);
    if tag == FORMAT_EXTENSIBLE {
        // The sub-format GUID's first two bytes carry the real tag.
        if body.len() < 26 {
            return Err(invalid("WAV extensible fmt chunk is too short"));
        }
        tag = u16_at(body, 24);
    }
    let sample = match (tag, bits) {
        (FORMAT_PCM, 16) => SampleFormat::Int16,
        (FORMAT_PCM, 24) => SampleFormat::Int24,
        (FORMAT_PCM, 32) => SampleFormat::Int32,
        (FORMAT_IEEE_FLOAT, 32) => SampleFormat::Float32,
        _ => return Err(invalid(format!("unsupported WAV format (tag {tag}, {bits}-bit)"))),
    };
    if channels == 0 || channels > MAX_CHANNELS {
        return Err(invalid(format!("only mono or stereo WAVs can be exported ({channels} channels)")));
    }
    if sample_rate == 0 {
        return Err(invalid("WAV sample rate is 0"));
    }
    Ok(WavFormat {
        channels,
        sample_rate,
        sample,
    })
}

impl<R: Read + Seek> WavReader<R> {
    pub fn new(mut inner: R) -> io::Result<Self> {
        let mut riff = [0u8; 12];
        inner.read_exact(&mut riff).map_err(|_| invalid("not a WAV file"))?;
        if &riff[0..4] != b"RIFF" || &riff[8..12] != b"WAVE" {
            return Err(invalid("not a WAV file"));
        }
        let mut format = None;
        loop {
            let mut head = [0u8; 8];
            inner
                .read_exact(&mut head)
                .map_err(|_| invalid("WAV file has no data chunk"))?;
            let size = u32_at(&head, 4) as u64;
            match &head[0..4] {
                b"fmt " => {
                    let mut body = vec![0u8; size as usize];
                    inner.read_exact(&mut body)?;
                    format = Some(parse_fmt(&body)?);
                    if size % 2 == 1 {
                        inner.seek(SeekFrom::Current(1))?;
                    }
                }
                b"data" => {
                    let format = format.ok_or_else(|| invalid("WAV data comes before fmt"))?;
                    let start = inner.stream_position()?;
                    let end = inner.seek(SeekFrom::End(0))?;
                    inner.seek(SeekFrom::Start(start))?;
                    let remaining = end.saturating_sub(start);
                    let data_bytes = if size == 0 || size > remaining { remaining } else { size };
                    let frames = data_bytes / format.block_align() as u64;
                    return Ok(WavReader {
                        inner,
                        format,
                        frames_left: frames,
                        total_frames: frames,
                        bytes: Vec::new(),
                    });
                }
                _ => {
                    inner.seek(SeekFrom::Current((size + size % 2) as i64))?;
                }
            }
        }
    }

    pub fn format(&self) -> WavFormat {
        self.format
    }

    pub fn total_frames(&self) -> u64 {
        self.total_frames
    }

    /// Read up to `max_frames` frames as interleaved f32 into `out`
    /// (cleared first). Returns the frames read; 0 at the end.
    pub fn read_frames(&mut self, max_frames: usize, out: &mut Vec<f32>) -> io::Result<usize> {
        out.clear();
        let frames = (max_frames as u64).min(self.frames_left) as usize;
        if frames == 0 {
            return Ok(0);
        }
        let len = frames * self.format.block_align();
        self.bytes.resize(len, 0);
        let got = read_up_to(&mut self.inner, &mut self.bytes)?;
        let frames = got / self.format.block_align();
        let used = frames * self.format.block_align();
        decode(self.format.sample, &self.bytes[..used], out);
        self.frames_left = if frames == 0 { 0 } else { self.frames_left - frames as u64 };
        Ok(frames)
    }
}

/// Fill `buf` as far as the reader allows (stops early only at EOF).
fn read_up_to<R: Read>(reader: &mut R, buf: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// Little-endian samples → f32 (integers scaled to ±1).
pub fn decode(sample: SampleFormat, bytes: &[u8], out: &mut Vec<f32>) {
    match sample {
        SampleFormat::Int16 => out.extend(
            bytes
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32_768.0),
        ),
        SampleFormat::Int24 => out.extend(bytes.chunks_exact(3).map(|b| {
            let v = i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8;
            v as f32 / 8_388_608.0
        })),
        SampleFormat::Int32 => out.extend(
            bytes
                .chunks_exact(4)
                .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0),
        ),
        SampleFormat::Float32 => out.extend(
            bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recorder::wav::header_bytes;
    use std::io::Cursor;

    fn pcm16_wav(channels: u16, rate: u32, samples: &[i16], extra_chunk: bool) -> Vec<u8> {
        let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF\0\0\0\0WAVE");
        if extra_chunk {
            out.extend_from_slice(b"LIST\x03\0\0\0abc\0");
        }
        out.extend_from_slice(b"fmt \x10\0\0\0");
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
        out.extend_from_slice(&(channels * 2).to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&data);
        out
    }

    #[test]
    fn reads_the_recorders_float_tracks() {
        let samples = [0.5f32, -0.25, 1.5, 0.0];
        let mut bytes = header_bytes(2, 48_000, 16).to_vec();
        bytes.extend(samples.iter().flat_map(|s| s.to_le_bytes()));
        let mut reader = WavReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(
            reader.format(),
            WavFormat { channels: 2, sample_rate: 48_000, sample: SampleFormat::Float32 }
        );
        assert_eq!(reader.total_frames(), 2);
        let mut out = Vec::new();
        assert_eq!(reader.read_frames(10, &mut out).unwrap(), 2);
        assert_eq!(out, samples);
        assert_eq!(reader.read_frames(10, &mut out).unwrap(), 0);
        assert!(out.is_empty());
    }

    #[test]
    fn an_unpatched_header_reads_to_the_end_of_the_file() {
        // Sizes still 0, as after a power cut before the first patch.
        let mut bytes = header_bytes(1, 48_000, 0).to_vec();
        bytes.extend([0.1f32, 0.2, 0.3].iter().flat_map(|s| s.to_le_bytes()));
        bytes.push(0xAA); // a torn last sample is ignored
        let mut reader = WavReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(reader.total_frames(), 3);
        let mut out = Vec::new();
        assert_eq!(reader.read_frames(2, &mut out).unwrap(), 2);
        assert_eq!(out, vec![0.1, 0.2]);
        assert_eq!(reader.read_frames(2, &mut out).unwrap(), 1);
        assert_eq!(out, vec![0.3]);
    }

    #[test]
    fn reads_pcm16_and_skips_unknown_chunks() {
        let bytes = pcm16_wav(1, 44_100, &[16_384, -32_768, 0], true);
        let mut reader = WavReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(reader.format().sample, SampleFormat::Int16);
        assert_eq!(reader.format().sample_rate, 44_100);
        let mut out = Vec::new();
        reader.read_frames(8, &mut out).unwrap();
        assert_eq!(out, vec![0.5, -1.0, 0.0]);
    }

    #[test]
    fn decodes_24_and_32_bit_integers() {
        let mut out = Vec::new();
        decode(SampleFormat::Int24, &[0x00, 0x00, 0x40, 0x00, 0x00, 0x80], &mut out);
        assert_eq!(out, vec![0.5, -1.0]);
        out.clear();
        decode(SampleFormat::Int32, &(i32::MIN).to_le_bytes(), &mut out);
        assert_eq!(out, vec![-1.0]);
    }

    #[test]
    fn extensible_float_format_is_understood() {
        let mut body = vec![0u8; 40];
        body[0..2].copy_from_slice(&FORMAT_EXTENSIBLE.to_le_bytes());
        body[2..4].copy_from_slice(&2u16.to_le_bytes());
        body[4..8].copy_from_slice(&96_000u32.to_le_bytes());
        body[14..16].copy_from_slice(&32u16.to_le_bytes());
        body[24..26].copy_from_slice(&FORMAT_IEEE_FLOAT.to_le_bytes());
        let format = parse_fmt(&body).unwrap();
        assert_eq!(format.sample, SampleFormat::Float32);
        assert_eq!(format.sample_rate, 96_000);
    }

    #[test]
    fn rejects_what_it_cannot_export() {
        assert!(WavReader::new(Cursor::new(b"RIFX0000WAVE".to_vec())).is_err());
        let mut body = vec![0u8; 16];
        body[0..2].copy_from_slice(&1u16.to_le_bytes());
        body[2..4].copy_from_slice(&6u16.to_le_bytes());
        body[4..8].copy_from_slice(&48_000u32.to_le_bytes());
        body[14..16].copy_from_slice(&16u16.to_le_bytes());
        let err = parse_fmt(&body).unwrap_err().to_string();
        assert!(err.contains("mono or stereo"), "{err}");
        body[2..4].copy_from_slice(&1u16.to_le_bytes());
        body[14..16].copy_from_slice(&8u16.to_le_bytes());
        assert!(parse_fmt(&body).is_err(), "8-bit is not supported");
    }
}

use super::mapping::{matching_input, warning, MappingWarning, PlaybackInput};
use crate::export::request::is_plain_file_name;
use crate::export::wav_read::WavReader;
use crate::mixer_control::MAX_STRIPS;
use crate::recorder::session::{
    Marker, SessionFile, SessionTrack, TrackKind, SESSION_FILE, SESSION_FORMAT,
    SESSION_FORMAT_VERSION,
};
use serde::Serialize;
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_PARTS_PER_TRACK: usize = 64;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    pub index: usize,
    pub name: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub title: String,
    pub source_rate: u32,
    pub rate: u32,
    pub frames: u64,
    pub tracks: Vec<TrackInfo>,
    pub markers: Vec<Marker>,
    pub dropped_frames: u64,
    pub warnings: Vec<MappingWarning>,
}

struct Part {
    start: u64,
    end: u64,
    wav: WavReader<BufReader<File>>,
}

struct Track {
    parts: Vec<Part>,
    cache: Vec<f32>,
    cache_start: u64,
    cache_frames: u64,
    first_read: u64,
    prior_last: Option<(u64, f32)>,
}

impl Track {
    fn open(folder: &Path, track: &SessionTrack, rate: u32, index: usize) -> Result<Self, String> {
        let names = if track.files.is_empty() {
            std::slice::from_ref(&track.file)
        } else {
            &track.files
        };
        if names.len() > MAX_PARTS_PER_TRACK || names.first() != Some(&track.file) {
            return Err("Invalid recording parts".into());
        }

        let mut seen = HashSet::new();
        let mut end = 0u64;
        let mut parts = Vec::with_capacity(names.len());
        for name in names {
            if !seen.insert(name) {
                return Err("Duplicate recording part".into());
            }
            let path = local_file(folder, name)?;
            let file = File::open(path).map_err(|_| "Cannot open recorded track")?;
            let wav = WavReader::new(BufReader::new(file)).map_err(|_| "Invalid track WAV")?;
            if wav.format().sample_rate != rate || wav.format().channels != 1 || track.channels != 1
            {
                return Err("Incompatible track WAV".into());
            }
            let start = end;
            end = end
                .checked_add(wav.total_frames())
                .ok_or("Session is too long")?;
            parts.push(Part { start, end, wav });
        }
        let cache_frames = (rate as u64 / 3).clamp(8192, 65536);
        Ok(Self {
            parts,
            cache: Vec::with_capacity(cache_frames as usize),
            cache_frames,
            first_read: cache_frames - (cache_frames - 2048) * index as u64 / MAX_STRIPS as u64,
            cache_start: 0,
            prior_last: None,
        })
    }

    fn frames(&self) -> u64 {
        self.parts.last().map_or(0, |part| part.end)
    }

    fn sample(&mut self, frame: u64) -> io::Result<f32> {
        if frame >= self.frames() {
            return Ok(0.0);
        }
        if let Some(index) = frame.checked_sub(self.cache_start) {
            if let Some(sample) = self.cache.get(index as usize) {
                return Ok(*sample);
            }
        }
        if let Some((index, sample)) = self.prior_last {
            if index == frame {
                return Ok(sample);
            }
        }
        let part = self
            .parts
            .iter_mut()
            .find(|part| frame < part.end)
            .ok_or_else(|| io::Error::other("Missing recording part"))?;
        // Keep the old block's last sample for interpolation across a cache or rollover boundary.
        self.prior_last = self
            .cache
            .last()
            .map(|&sample| (self.cache_start + self.cache.len() as u64 - 1, sample));
        let local = frame - part.start;
        let (cache_frame, block) = if local < self.first_read {
            (0, self.first_read)
        } else {
            (
                self.first_read + (local - self.first_read) / self.cache_frames * self.cache_frames,
                self.cache_frames,
            )
        };
        part.wav.seek_frame(cache_frame)?;
        let expected = (part.end - part.start - cache_frame).min(block) as usize;
        if part.wav.read_frames(expected, &mut self.cache)? != expected {
            return Err(io::Error::other("Recording part changed during playback"));
        }
        self.cache_start = part.start + cache_frame;
        Ok(self.cache[(local - cache_frame) as usize])
    }
}

fn local_file(folder: &Path, name: &str) -> Result<std::path::PathBuf, String> {
    if !is_plain_file_name(name) {
        return Err("File is outside the session".into());
    }
    let path = folder
        .join(name)
        .canonicalize()
        .map_err(|_| "Missing session file")?;
    if path.parent() != Some(folder) || !path.is_file() {
        return Err("File is outside the session".into());
    }
    Ok(path)
}

pub struct Source {
    pub info: SessionInfo,
    tracks: Vec<(usize, Track)>,
}

impl Source {
    pub fn frame(&mut self, frame: u64) -> io::Result<[f32; MAX_STRIPS]> {
        let scaled = frame * u64::from(self.info.source_rate);
        let rate = u64::from(self.info.rate);
        let base = scaled / rate;
        let fraction = (scaled % rate) as f32 / rate as f32;
        let mut samples = [0.0; MAX_STRIPS];
        for (index, track) in &mut self.tracks {
            let clean = |sample: f32| if sample.is_finite() { sample } else { 0.0 };
            let left = clean(track.sample(base)?);
            samples[*index] = if fraction == 0.0 {
                left
            } else {
                left * (1.0 - fraction) + clean(track.sample(base + 1)?) * fraction
            };
        }
        Ok(samples)
    }
}

pub fn open_session(path: &Path, inputs: &[PlaybackInput], rate: u32) -> Result<Source, String> {
    if path.file_name().and_then(|name| name.to_str()) != Some(SESSION_FILE) {
        return Err("Choose session.json".into());
    }
    let folder = path
        .parent()
        .and_then(|folder| folder.canonicalize().ok())
        .ok_or("Missing session folder")?;
    let file = File::open(local_file(&folder, SESSION_FILE)?).map_err(|_| "Cannot open session")?;
    let mut bytes = Vec::new();
    file.take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read session")?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("Session manifest is too large".into());
    }
    let manifest: SessionFile =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid session manifest")?;
    if manifest.format != SESSION_FORMAT
        || manifest.format_version != SESSION_FORMAT_VERSION
        || !(1..=384_000).contains(&manifest.sample_rate)
        || !(1..=384_000).contains(&rate)
    {
        return Err("Unsupported session format".into());
    }

    let mut tracks = Vec::new();
    let mut mapped = Vec::new();
    let mut indices = HashSet::new();
    let mut length = 0;
    let mut used = HashSet::new();
    let mut warnings = Vec::new();
    for track in manifest
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Strip)
    {
        let index = track.strip.ok_or("Missing strip mapping")? as usize;
        if !indices.insert(index) {
            return Err("Duplicate strip mapping".into());
        }
        let input = match matching_input(track, inputs, index) {
            Ok(input) if input.index < MAX_STRIPS && used.insert(input.index) => input,
            Ok(_) => {
                warnings.push(warning(track, "Destination already used"));
                continue;
            }
            Err(reason) => {
                warnings.push(reason);
                continue;
            }
        };
        let index = input.index;
        let source = Track::open(&folder, track, manifest.sample_rate, index)?;
        length = length.max(source.frames());
        mapped.push(TrackInfo {
            index,
            name: track.name.clone(),
        });
        tracks.push((index, source));
    }
    if length == 0 {
        return Err("No playable tracks match the active strip names and source kinds".into());
    }
    let frames = length
        .checked_mul(u64::from(rate))
        .ok_or("Session is too long")?
        .div_ceil(u64::from(manifest.sample_rate));
    if frames > super::transport::MAX_POSITION {
        return Err("Session is too long".into());
    }
    let title = manifest.title.unwrap_or_else(|| {
        folder.file_name().map_or_else(
            || "Session".into(),
            |name| name.to_string_lossy().into_owned(),
        )
    });
    Ok(Source {
        info: SessionInfo {
            title,
            rate,
            source_rate: manifest.sample_rate,
            frames,
            tracks: mapped,
            markers: manifest.markers,
            dropped_frames: manifest.dropped_frames,
            warnings,
        },
        tracks,
    })
}

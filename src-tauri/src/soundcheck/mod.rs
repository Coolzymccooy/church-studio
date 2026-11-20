mod session;
mod transport;

pub use session::{open_session, SessionInfo, Source};
use transport::Transport;

use crate::dsp::mixer::smooth::{ramp_samples, LinearSmoother};
use crate::mixer_control::MAX_STRIPS;
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};
use serde::Serialize;
use std::sync::atomic::{
    AtomicBool, AtomicU32, AtomicU64,
    Ordering::{Acquire, Relaxed, Release},
};
use std::sync::{mpsc, Arc};
use std::time::Duration;

const CHUNK_FRAMES: usize = 128;
const CHUNKS: usize = 64;
const POLL: Duration = Duration::from_millis(2);

struct Chunk {
    session: u64,
    epoch: u64,
    start: u64,
    len: usize,
    samples: [[f32; MAX_STRIPS]; CHUNK_FRAMES],
}

#[derive(Default)]
struct Shared {
    transport: Transport,
    session: AtomicU64,
    frames: AtomicU64,
    selected: AtomicU32,
    buffering: AtomicBool,
    failed_session: AtomicU64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackStatus {
    session: Option<SessionInfo>,
    selected: u32,
    seconds: f64,
    duration: f64,
    playing: bool,
    looping: bool,
    buffering: bool,
    error: Option<&'static str>,
}

impl PlaybackStatus {
    pub fn stopped() -> Self {
        Self {
            session: None,
            selected: 0,
            seconds: 0.0,
            duration: 0.0,
            playing: false,
            looping: false,
            buffering: false,
            error: None,
        }
    }
}

enum Command {
    Load(u64, Source),
    Clear,
    Wake,
}

pub struct PlaybackPort {
    tx: mpsc::Sender<Command>,
    info: Option<SessionInfo>,
    shared: Arc<Shared>,
    // Identifies this running engine without holding its mutex during file I/O.
    pub identity: Arc<()>,
}

impl PlaybackPort {
    pub fn load(&mut self, source: Source) -> Result<(), String> {
        let id = self.shared.session.load(Relaxed).wrapping_add(1);
        let info = source.info.clone();
        self.tx
            .send(Command::Load(id, source))
            .map_err(|_| "Soundcheck worker stopped")?;
        self.reset(Some(info), id);
        Ok(())
    }

    pub fn unload(&mut self) -> Result<(), String> {
        self.tx
            .send(Command::Clear)
            .map_err(|_| "Soundcheck worker stopped")?;
        let id = self.shared.session.load(Relaxed).wrapping_add(1);
        self.reset(None, id);
        Ok(())
    }

    fn reset(&mut self, info: Option<SessionInfo>, id: u64) {
        self.shared
            .transport
            .update(|cursor| cursor.move_to(0, false).with_loop(false));
        self.shared.selected.store(0, Relaxed);
        self.shared
            .frames
            .store(info.as_ref().map_or(0, |info| info.frames), Relaxed);
        self.shared.session.store(id, Release);
        self.info = info;
    }

    fn session(&self) -> Result<&SessionInfo, String> {
        self.info
            .as_ref()
            .ok_or_else(|| "Load a session first".into())
    }

    pub fn set_source(&self, index: usize, playback: bool) -> Result<(), String> {
        if !self
            .session()?
            .tracks
            .iter()
            .any(|track| track.index == index)
        {
            return Err("Strip has no playback track".into());
        }
        if playback {
            self.shared.selected.fetch_or(1 << index, Relaxed);
        } else {
            self.shared.selected.fetch_and(!(1 << index), Relaxed);
        }
        Ok(())
    }

    pub fn active(&self) -> bool {
        self.shared.transport.get().playing() || self.shared.selected.load(Relaxed) != 0
    }

    pub fn play(&self) -> Result<(), String> {
        let frames = self.session()?.frames;
        if self.failed() {
            return Err("Reload the soundcheck session".into());
        }
        self.shared.transport.update(|cursor| {
            if cursor.playing() {
                cursor
            } else {
                let source = if cursor.frame() == frames {
                    0
                } else {
                    cursor.frame()
                };
                cursor.move_to(source, true)
            }
        });
        self.tx
            .send(Command::Wake)
            .map_err(|_| "Soundcheck worker stopped".into())
    }

    pub fn stop(&self) -> Result<(), String> {
        self.session()?;
        self.shared
            .transport
            .update(|cursor| cursor.move_to(cursor.frame(), false));
        Ok(())
    }

    pub fn locate(&self, seconds: f64) -> Result<(), String> {
        let session = self.session()?;
        if !seconds.is_finite() {
            return Err("Invalid locate time".into());
        }
        let frame = (seconds * f64::from(session.rate))
            .round()
            .clamp(0.0, session.frames as f64) as u64;
        self.shared
            .transport
            .update(|cursor| cursor.move_to(frame, cursor.playing() && frame < session.frames));
        Ok(())
    }

    pub fn set_loop(&self, enabled: bool) -> Result<(), String> {
        self.session()?;
        self.shared.transport.update(|cursor| {
            if cursor.looping() == enabled {
                cursor
            } else {
                cursor.with_loop(enabled)
            }
        });
        Ok(())
    }

    fn failed(&self) -> bool {
        self.shared.failed_session.load(Relaxed) == self.shared.session.load(Relaxed)
    }

    pub fn status(&self) -> PlaybackStatus {
        let Some(info) = &self.info else {
            return PlaybackStatus::stopped();
        };
        let cursor = self.shared.transport.get();
        let rate = f64::from(info.rate);
        PlaybackStatus {
            session: Some(info.clone()),
            selected: self.shared.selected.load(Relaxed),
            seconds: cursor.frame() as f64 / rate,
            duration: info.frames as f64 / rate,
            playing: cursor.playing(),
            looping: cursor.looping(),
            buffering: cursor.playing() && self.shared.buffering.load(Relaxed),
            error: self.failed().then_some("Playback read failed"),
        }
    }
}

pub struct PlaybackTap {
    shared: Arc<Shared>,
    fifo: HeapCons<Chunk>,
    chunk: Option<Chunk>,
    gains: [LinearSmoother; MAX_STRIPS],
    channels: usize,
    fading: u32,
}

impl PlaybackTap {
    pub fn process(&mut self, inputs: &mut [Vec<f32>], frames: usize) {
        let id = self.shared.session.load(Acquire);
        let cursor = self.shared.transport.get();
        let selected = self.shared.selected.load(Relaxed);
        if !cursor.playing() && selected == 0 && self.fading == 0 {
            return;
        }
        let length = self.shared.frames.load(Relaxed);
        let mut pos = cursor.frame();
        let mut playing = cursor.playing() && pos < length;
        let mut buffering = false;
        let mut discards = 8;
        let channels = if selected | self.fading == 0 {
            0
        } else {
            self.channels.min(inputs.len())
        };
        for i in 0..channels {
            self.gains[i].set_target(f32::from(selected & (1 << i) != 0));
        }
        for i in 0..frames {
            let values = if playing {
                self.next_frame(id, cursor.epoch(), pos, &mut discards)
            } else {
                None
            };
            buffering |= playing && values.is_none();
            if values.is_some() {
                pos += 1;
                if pos == length {
                    if cursor.looping() {
                        pos = 0;
                    } else {
                        playing = false;
                    }
                }
            }
            for channel in 0..channels {
                if (selected | self.fading) & (1 << channel) == 0 {
                    continue;
                }
                let gain = self.gains[channel].next();
                let sample = &mut inputs[channel][i];
                let playback = values.as_ref().map_or(0.0, |frame| frame[channel]);
                *sample = if gain == 1.0 {
                    playback
                } else {
                    *sample * (1.0 - gain) + playback * gain
                };
            }
        }
        self.fading = (0..channels)
            .filter(|&i| self.gains[i].current() > 0.0)
            .fold(0, |mask, i| mask | (1 << i));
        if self
            .shared
            .transport
            .publish(cursor, cursor.progress(pos, playing))
        {
            self.shared.buffering.store(buffering, Relaxed);
        }
    }

    fn next_frame(
        &mut self,
        id: u64,
        epoch: u64,
        pos: u64,
        discards: &mut usize,
    ) -> Option<[f32; MAX_STRIPS]> {
        loop {
            if let Some(chunk) = self.chunk.as_ref() {
                if chunk.session == id && chunk.epoch == epoch {
                    if let Some(index) = pos
                        .checked_sub(chunk.start)
                        .filter(|i| *i < chunk.len as u64)
                    {
                        let sample = chunk.samples[index as usize];
                        if index as usize + 1 == chunk.len {
                            self.chunk = None;
                        }
                        return Some(sample);
                    }
                }
                self.chunk = None;
                *discards = discards.saturating_sub(1);
            }
            if *discards == 0 {
                return None;
            }
            self.chunk = Some(self.fifo.try_pop()?);
        }
    }
}

pub fn playback_channel(rate: u32, channels: usize) -> Result<(PlaybackTap, PlaybackPort), String> {
    let (producer, fifo) = HeapRb::new(CHUNKS).split();
    let (tx, rx) = mpsc::channel();
    let shared = Arc::new(Shared::default());
    let worker_shared = shared.clone();
    std::thread::Builder::new()
        .name("soundcheck".into())
        .spawn(move || worker(producer, rx, &worker_shared))
        .map_err(|_| "Cannot start soundcheck worker")?;
    let tap = PlaybackTap {
        shared: shared.clone(),
        fifo,
        chunk: None,
        gains: std::array::from_fn(|_| LinearSmoother::new(0.0, ramp_samples(f64::from(rate)))),
        channels: channels.min(MAX_STRIPS),
        fading: 0,
    };
    Ok((
        tap,
        PlaybackPort {
            tx,
            shared,
            info: None,
            identity: Arc::new(()),
        },
    ))
}

fn worker(mut fifo: HeapProd<Chunk>, rx: mpsc::Receiver<Command>, shared: &Shared) {
    let mut session = None;
    let mut epoch = None;
    let mut next = 0;
    loop {
        let command = if !shared.transport.get().playing() || session.is_none() {
            match rx.recv() {
                Ok(value) => Some(value),
                Err(_) => break,
            }
        } else if fifo.is_full() {
            match rx.recv_timeout(POLL) {
                Ok(value) => Some(value),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match rx.try_recv() {
                Ok(value) => Some(value),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        };
        if let Some(command) = command {
            match command {
                Command::Load(id, source) => {
                    session = Some((id, source));
                    epoch = None;
                }
                Command::Clear => session = None,
                Command::Wake => {}
            }
            continue;
        }
        let Some((id, source)) = &mut session else {
            continue;
        };
        let current = shared.session.load(Acquire);
        let cursor = shared.transport.get();
        if *id != current || !cursor.playing() || fifo.is_full() {
            continue;
        }
        if epoch != Some(cursor.epoch()) {
            epoch = Some(cursor.epoch());
            next = cursor.frame();
        }
        let len = source
            .info
            .frames
            .saturating_sub(next)
            .min(CHUNK_FRAMES as u64) as usize;
        if len == 0 {
            std::thread::sleep(POLL);
            continue;
        }
        let mut chunk = Chunk {
            session: *id,
            epoch: cursor.epoch(),
            start: next,
            len,
            samples: [[0.0; MAX_STRIPS]; CHUNK_FRAMES],
        };
        let filled = chunk.samples[..len]
            .iter_mut()
            .enumerate()
            .try_for_each(|(i, frame)| {
                source
                    .frame(next + i as u64)
                    .map(|samples| *frame = samples)
            });
        if filled.is_err() {
            shared.failed_session.store(*id, Relaxed);
            shared.transport.update(|now| {
                if now.epoch() == cursor.epoch() && shared.session.load(Relaxed) == *id {
                    now.move_to(now.frame(), false)
                } else {
                    now
                }
            });
            continue;
        }
        if fifo.try_push(chunk).is_ok() {
            next += len as u64;
            if next == source.info.frames && cursor.looping() {
                next = 0;
            }
        }
    }
}

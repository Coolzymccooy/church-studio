//! Link state of each engine stream and the reconnect decision
//! (`docs/specs/2026-11-20-usb-interfaces.md` §3).
//!
//! The cpal error callback and the device watcher write the state; the
//! watcher asks `Reconnector` what to do each tick. Pure apart from atomics.
use cpal::ErrorKind;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LinkState {
    Ok = 0,
    /// Chosen, but not connected when the engine started.
    Waiting = 1,
    /// Dropped out while running.
    Lost = 2,
    /// Back, but could not be reopened.
    Failed = 3,
}

impl LinkState {
    fn from_u8(value: u8) -> Self {
        match value {
            0 => LinkState::Ok,
            1 => LinkState::Waiting,
            2 => LinkState::Lost,
            _ => LinkState::Failed,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            LinkState::Ok => "ok",
            LinkState::Waiting => "waiting",
            LinkState::Lost => "lost",
            LinkState::Failed => "failed",
        }
    }
}

/// The state a stream error moves the stream to, if any. A disconnect, or a
/// sample-rate change made elsewhere (cpal invalidates the stream), both
/// need the stream reopened.
pub fn state_after_error(kind: ErrorKind) -> Option<LinkState> {
    match kind {
        ErrorKind::DeviceNotAvailable | ErrorKind::StreamInvalidated => Some(LinkState::Lost),
        _ => None,
    }
}

/// One stream's live state plus the last error text (control side only).
#[derive(Debug)]
pub struct StreamHealth {
    state: AtomicU8,
    error: Mutex<Option<String>>,
    /// The device id (Core Audio UID, WASAPI endpoint id) the stream was
    /// last opened on, to tell identical interfaces apart.
    bound: Mutex<Option<String>>,
}

impl StreamHealth {
    pub fn new(state: LinkState) -> Self {
        StreamHealth {
            state: AtomicU8::new(state as u8),
            error: Mutex::new(None),
            bound: Mutex::new(None),
        }
    }

    pub fn state(&self) -> LinkState {
        LinkState::from_u8(self.state.load(Ordering::Acquire))
    }

    pub fn set(&self, state: LinkState) {
        self.state.store(state as u8, Ordering::Release);
        if state == LinkState::Ok {
            *self.error.lock() = None;
        }
    }

    pub fn fail(&self, state: LinkState, error: impl Into<String>) {
        *self.error.lock() = Some(error.into());
        self.state.store(state as u8, Ordering::Release);
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().clone()
    }

    pub fn bound(&self) -> Option<String> {
        self.bound.lock().clone()
    }

    pub fn bind(&self, uid: Option<String>) {
        *self.bound.lock() = uid;
    }

    /// From a cpal error callback (not the audio thread).
    pub fn on_stream_error(&self, kind: ErrorKind, text: &str) {
        if let Some(state) = state_after_error(kind) {
            if self.state() == LinkState::Ok {
                self.fail(state, text.to_string());
            }
        }
    }
}

/// What the watcher should do for one stream this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Wait,
    /// Working, but no longer listed: mark it lost.
    MarkLost,
    /// Down and listed again long enough: reopen it now.
    Reopen,
}

/// Ticks a device must be listed again before it is reopened.
pub const LISTED_TICKS: u8 = 2;
/// Ticks a working device must be missing from the list before it is
/// marked lost (one failed listing is not an unplug).
pub const UNLISTED_TICKS: u8 = 2;
/// Reopen attempts per appearance of the device.
pub const MAX_ATTEMPTS: u8 = 3;

/// Per-stream reconnect bookkeeping for the watcher.
#[derive(Debug, Default, Clone)]
pub struct Reconnector {
    listed_ticks: u8,
    unlisted_ticks: u8,
    attempts: u8,
}

impl Reconnector {
    pub fn tick(&mut self, state: LinkState, listed: bool) -> Step {
        if !listed {
            self.listed_ticks = 0;
            self.attempts = 0;
            if state != LinkState::Ok {
                return Step::Wait;
            }
            self.unlisted_ticks = self.unlisted_ticks.saturating_add(1);
            return if self.unlisted_ticks >= UNLISTED_TICKS { Step::MarkLost } else { Step::Wait };
        }
        self.unlisted_ticks = 0;
        if state == LinkState::Ok {
            self.listed_ticks = 0;
            self.attempts = 0;
            return Step::Wait;
        }
        self.listed_ticks = self.listed_ticks.saturating_add(1);
        if self.listed_ticks < LISTED_TICKS || self.attempts >= MAX_ATTEMPTS {
            return Step::Wait;
        }
        self.attempts += 1;
        Step::Reopen
    }
}

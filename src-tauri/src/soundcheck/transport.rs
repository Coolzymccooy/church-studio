use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

const POSITION_BITS: u32 = 42;
const EPOCH_MASK: u64 = (1 << 20) - 1;
const PLAYING: u64 = 1 << 62;
const LOOP: u64 = 1 << 63;
pub(super) const MAX_POSITION: u64 = (1 << POSITION_BITS) - 1;

#[derive(Clone, Copy)]
pub(super) struct Cursor(pub u64);

impl Cursor {
    pub fn frame(self) -> u64 {
        self.0 & MAX_POSITION
    }
    pub fn epoch(self) -> u64 {
        (self.0 >> POSITION_BITS) & EPOCH_MASK
    }
    pub fn playing(self) -> bool {
        self.0 & PLAYING != 0
    }
    pub fn looping(self) -> bool {
        self.0 & LOOP != 0
    }

    pub fn move_to(self, frame: u64, playing: bool) -> Self {
        let epoch = (self.epoch() + 1) & EPOCH_MASK;
        Self((epoch << POSITION_BITS) | (self.0 & LOOP)).progress(frame, playing)
    }

    pub fn with_loop(self, enabled: bool) -> Self {
        let flag = if enabled { LOOP } else { 0 };
        Self((self.0 & !LOOP) | flag).move_to(self.frame(), self.playing())
    }

    pub fn progress(self, frame: u64, playing: bool) -> Self {
        let flag = if playing { PLAYING } else { 0 };
        Self((self.0 & !(MAX_POSITION | PLAYING)) | frame | flag)
    }
}

// Control and audio both update this word. A callback that races a seek or
// stop can finish its old block, but its compare-exchange cannot undo the command.
#[derive(Default)]
pub(super) struct Transport(AtomicU64);

impl Transport {
    pub fn get(&self) -> Cursor {
        Cursor(self.0.load(Relaxed))
    }

    pub fn update(&self, change: impl Fn(Cursor) -> Cursor) {
        let mut old = self.0.load(Relaxed);
        loop {
            match self
                .0
                .compare_exchange_weak(old, change(Cursor(old)).0, Relaxed, Relaxed)
            {
                Ok(_) => return,
                Err(actual) => old = actual,
            }
        }
    }

    pub fn publish(&self, before: Cursor, after: Cursor) -> bool {
        self.0
            .compare_exchange(before.0, after.0, Relaxed, Relaxed)
            .is_ok()
    }
}

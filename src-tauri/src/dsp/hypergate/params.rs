//! Hyper gate controls shared between the Tauri commands and the audio
//! thread. All atomics: the control side stores, the callback loads.
//!
//! "Learn mic" handshake:
//! 1. The command bumps `learn_request` (`request_learn`).
//! 2. The voice chain sees the new request at its next block and learns for
//!    five seconds on the audio thread (no allocation, no locks).
//! 3. It stores the fingerprint here (`publish`), then the outcome code,
//!    then sets `learn_done` to the request number. The command polls
//!    `learn_result`. With the gate off or the chain bypassed the chain
//!    answers `GateOff` at once.
//! 4. A request the command gave up on (`abandon`) is never started, and is
//!    cancelled if it already was, so it cannot install a fingerprint later.
//!
//! The fingerprint lives here, not in the chain, so it survives an engine
//! restart: a new chain installs whatever `fp_generation` points at.
use super::fingerprint::{Fingerprint, LearnOutcome, FP_LEN};
use atomic_float::AtomicF32;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::time::{Duration, Instant};

pub const FP_FLAT_LEN: usize = 2 * FP_LEN;

/// How a learn request ended, seen from the control thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LearnWait {
    Learned,
    NotEnoughVoice,
    /// Gate off or chain bypassed.
    GateOff,
    /// The chain never answered: no live audio on the voice strip.
    TimedOut,
}

const CODE_LEARNED: u8 = 0;
const CODE_NOT_ENOUGH_VOICE: u8 = 1;
const CODE_GATE_OFF: u8 = 2;

#[derive(Default)]
pub struct HyperGateParams {
    /// Speech-only mode (false = balanced).
    pub speech_only: AtomicBool,
    learn_request: AtomicU32,
    learn_done: AtomicU32,
    learn_code: AtomicU8,
    learn_abandoned: AtomicU32,
    fp_valid: AtomicBool,
    fp_generation: AtomicU32,
    fp: [AtomicF32; FP_FLAT_LEN],
}

impl HyperGateParams {
    /// Control thread: ask the chain to learn; returns the request number.
    pub fn request_learn(&self) -> u32 {
        self.learn_request.fetch_add(1, Ordering::AcqRel).wrapping_add(1)
    }

    /// Latest learn request (audio thread).
    pub fn learn_request(&self) -> u32 {
        self.learn_request.load(Ordering::Acquire)
    }

    /// Control thread: None while `request` is still pending, else how it
    /// ended.
    pub fn learn_result(&self, request: u32) -> Option<LearnWait> {
        if self.learn_done.load(Ordering::Acquire) != request {
            return None;
        }
        Some(match self.learn_code.load(Ordering::Acquire) {
            CODE_LEARNED => LearnWait::Learned,
            CODE_GATE_OFF => LearnWait::GateOff,
            _ => LearnWait::NotEnoughVoice,
        })
    }

    /// Control thread: give up on `request` (after a timeout).
    pub fn abandon(&self, request: u32) {
        self.learn_abandoned.store(request, Ordering::Release);
    }

    /// Audio thread: true when the command gave up on `request`.
    pub fn is_abandoned(&self, request: u32) -> bool {
        self.learn_abandoned.load(Ordering::Acquire) == request
    }

    /// Audio thread: record the outcome of learn request `request`.
    pub fn publish(&self, request: u32, outcome: &LearnOutcome) {
        let code = match outcome {
            LearnOutcome::Learned(fp) => {
                let mut flat = [0.0f32; FP_FLAT_LEN];
                fp.to_flat(&mut flat);
                for (dst, v) in self.fp.iter().zip(flat.iter()) {
                    dst.store(*v, Ordering::Relaxed);
                }
                self.fp_valid.store(true, Ordering::Release);
                self.fp_generation.fetch_add(1, Ordering::AcqRel);
                CODE_LEARNED
            }
            LearnOutcome::NotEnoughVoice => CODE_NOT_ENOUGH_VOICE,
            LearnOutcome::GateOff => CODE_GATE_OFF,
        };
        self.learn_code.store(code, Ordering::Release);
        self.learn_done.store(request, Ordering::Release);
    }

    /// Changes whenever a fingerprint is learned or cleared.
    pub fn fingerprint_generation(&self) -> u32 {
        self.fp_generation.load(Ordering::Acquire)
    }

    /// The stored fingerprint, if one has been learned.
    pub fn fingerprint(&self) -> Option<Fingerprint> {
        if !self.fp_valid.load(Ordering::Acquire) {
            return None;
        }
        let mut flat = [0.0f32; FP_FLAT_LEN];
        for (dst, src) in flat.iter_mut().zip(self.fp.iter()) {
            *dst = src.load(Ordering::Relaxed);
        }
        Fingerprint::from_flat(&flat)
    }

    /// Control thread: block until `request` finishes or `timeout` passes
    /// (then the request is abandoned), polling every `poll`. Never call
    /// this on the audio thread.
    pub fn wait_for_learn(&self, request: u32, timeout: Duration, poll: Duration) -> LearnWait {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(result) = self.learn_result(request) {
                return result;
            }
            if Instant::now() >= deadline {
                self.abandon(request);
                return LearnWait::TimedOut;
            }
            std::thread::sleep(poll);
        }
    }

    /// Forget the learned fingerprint.
    #[allow(dead_code)] // no UI for it yet; used by tests
    pub fn clear_fingerprint(&self) {
        self.fp_valid.store(false, Ordering::Release);
        self.fp_generation.fetch_add(1, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learn_handshake_and_fingerprint_store() {
        let p = HyperGateParams::default();
        assert_eq!(p.fingerprint(), None);
        let req = p.request_learn();
        assert_eq!(p.learn_request(), req);
        assert_eq!(p.learn_result(req), None);

        let fp = Fingerprint { mean: [1.0; FP_LEN], std: [0.5; FP_LEN] };
        let gen = p.fingerprint_generation();
        p.publish(req, &LearnOutcome::Learned(fp));
        assert_eq!(p.learn_result(req), Some(LearnWait::Learned));
        assert_eq!(p.fingerprint(), Some(fp));
        assert_ne!(p.fingerprint_generation(), gen);

        let req2 = p.request_learn();
        p.publish(req2, &LearnOutcome::NotEnoughVoice);
        assert_eq!(p.learn_result(req2), Some(LearnWait::NotEnoughVoice));
        assert_eq!(p.fingerprint(), Some(fp), "a failed relearn keeps the old fingerprint");

        p.clear_fingerprint();
        assert_eq!(p.fingerprint(), None);
    }

    #[test]
    fn wait_reports_each_outcome() {
        let p = HyperGateParams::default();
        let tick = Duration::from_millis(1);
        let req = p.request_learn();
        assert!(!p.is_abandoned(req));
        assert_eq!(p.wait_for_learn(req, Duration::from_millis(5), tick), LearnWait::TimedOut);
        assert!(p.is_abandoned(req), "a timed-out request is abandoned");
        p.publish(req, &LearnOutcome::NotEnoughVoice);
        assert_eq!(p.wait_for_learn(req, tick, tick), LearnWait::NotEnoughVoice);
        let req = p.request_learn();
        let fp = Fingerprint { mean: [0.0; FP_LEN], std: [1.0; FP_LEN] };
        p.publish(req, &LearnOutcome::Learned(fp));
        assert_eq!(p.wait_for_learn(req, tick, tick), LearnWait::Learned);
        let req = p.request_learn();
        p.publish(req, &LearnOutcome::GateOff);
        assert_eq!(p.wait_for_learn(req, tick, tick), LearnWait::GateOff);
    }
}

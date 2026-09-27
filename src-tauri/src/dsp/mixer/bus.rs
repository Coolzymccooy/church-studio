//! Stereo output bus: fader, mute, brick-wall limiter, peak/RMS meters.
//!
//! The limiter (one `compressor::Limiter` per channel, 5 ms lookahead) always
//! runs so the bus latency never changes; when disabled its ceiling is raised
//! far above full scale so it never acts.
use super::params::{clamp_or, fader_db_to_lin, BusParams};
use super::smooth::LinearSmoother;
use super::strip::lin_to_db;
use super::StereoBuffer;
use crate::dsp::compressor::Limiter;
use std::sync::atomic::Ordering::Relaxed;

/// Ceiling used when the limiter is switched off (+60 dBFS: never reached).
const LIMITER_OFF_DB: f32 = 60.0;

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct BusMeters {
    pub peak_l_db: f32,
    pub peak_r_db: f32,
    pub rms_l_db: f32,
    pub rms_r_db: f32,
}

impl BusMeters {
    fn silent() -> Self {
        BusMeters {
            peak_l_db: -120.0,
            peak_r_db: -120.0,
            rms_l_db: -120.0,
            rms_r_db: -120.0,
        }
    }
}

pub struct Bus {
    fader: LinearSmoother,
    mute: LinearSmoother,
    limiter_l: Limiter,
    limiter_r: Limiter,
    ceiling_db: f32,
    meters: BusMeters,
}

impl Bus {
    pub fn new(sr: f64, ramp: u32) -> Self {
        Bus {
            fader: LinearSmoother::new(1.0, ramp),
            mute: LinearSmoother::new(1.0, ramp),
            limiter_l: Limiter::new(sr),
            limiter_r: Limiter::new(sr),
            ceiling_db: f32::NAN, // forces the first update
            meters: BusMeters::silent(),
        }
    }

    pub fn meters(&self) -> BusMeters {
        self.meters
    }

    /// Limiter lookahead in samples (the limiter always runs).
    pub fn latency_samples(&self) -> usize {
        self.limiter_l.latency_samples()
    }

    /// Apply fader, mute and limiter to the first `frames` samples of `buf`
    /// in place, then update the meters.
    pub fn process(&mut self, buf: &mut StereoBuffer, frames: usize, p: &BusParams, snap: bool) {
        let n = frames.min(buf.left.len()).min(buf.right.len());

        let fader = fader_db_to_lin(p.fader_db.load(Relaxed));
        let mute = if p.mute.load(Relaxed) { 0.0 } else { 1.0 };
        if snap {
            self.fader.snap(fader);
            self.mute.snap(mute);
        } else {
            self.fader.set_target(fader);
            self.mute.set_target(mute);
        }

        let ceiling = if p.limiter_enabled.load(Relaxed) {
            clamp_or(p.limiter_ceiling_db.load(Relaxed), -30.0, 0.0, -1.0)
        } else {
            LIMITER_OFF_DB
        };
        if ceiling != self.ceiling_db {
            self.limiter_l.set_threshold_db(ceiling);
            self.limiter_r.set_threshold_db(ceiling);
            self.ceiling_db = ceiling;
        }

        for i in 0..n {
            let g = self.fader.next() * self.mute.next();
            buf.left[i] *= g;
            buf.right[i] *= g;
        }

        self.limiter_l.process_block(&mut buf.left[..n]);
        self.limiter_r.process_block(&mut buf.right[..n]);

        self.meters = BusMeters {
            peak_l_db: lin_to_db(peak(&buf.left[..n])),
            peak_r_db: lin_to_db(peak(&buf.right[..n])),
            rms_l_db: lin_to_db(rms(&buf.left[..n])),
            rms_r_db: lin_to_db(rms(&buf.right[..n])),
        };
    }
}

fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0f32, |m, &s| m.max(s.abs()))
}

fn rms(buf: &[f32]) -> f32 {
    if buf.is_empty() {
        return 0.0;
    }
    let sum: f64 = buf.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum / buf.len() as f64).sqrt() as f32
}

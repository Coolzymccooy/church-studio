//! One mono input channel strip.
//!
//! Signal flow:
//!   input → trim × polarity (ramped) → HPF → gate (lookahead, always delays)
//!         → 3-band EQ → compressor → [pre-fader] → fader (ramped) → [post-fader]
//!         → pan (constant power, ramped) × send (ramped) × mute (ramped) → buses
//!
//! Main and Stream sends are post-fader. The Monitor send is pre-fader unless
//! `monitor_post_fader` is set. When any strip is soloed the Monitor bus
//! carries only soloed strips, pre-fader at unity (PFL); Main and Stream are
//! never affected by solo. Mute removes the strip from every bus.
//!
//! Latency: every strip carries the gate's 20 ms lookahead whether or not the
//! gate is enabled, so all strips stay phase-aligned.
use super::params::{
    clamp_or, fader_db_to_lin, StripParams, BUS_MAIN, BUS_MONITOR, BUS_STREAM, EQ_MAX_DB,
    NUM_BUSES, TRIM_MAX_DB, TRIM_MIN_DB,
};
use super::smooth::LinearSmoother;
use super::StereoBuffer;
use crate::dsp::biquad::Biquad;
use crate::dsp::compressor::Compressor;
use crate::dsp::gate::Gate;
use std::sync::atomic::Ordering::Relaxed;

const EQ_LOW_HZ: f64 = 100.0;
const EQ_MID_HZ: f64 = 1_000.0;
const EQ_MID_Q: f64 = 0.9;
const EQ_HIGH_HZ: f64 = 8_000.0;

/// Per-strip meter readings for the last processed block.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct StripMeters {
    /// Peak before the fader, dBFS.
    pub pre_fader_peak_db: f32,
    /// Peak after the fader (before pan/sends), dBFS.
    pub post_fader_peak_db: f32,
    pub gate_open: bool,
    /// Compressor gain reduction, dB (≤ 0).
    pub gain_reduction_db: f32,
}

impl StripMeters {
    fn silent() -> Self {
        StripMeters {
            pre_fader_peak_db: -120.0,
            post_fader_peak_db: -120.0,
            gate_open: false,
            gain_reduction_db: 0.0,
        }
    }
}

pub struct Strip {
    sr: f64,
    scratch: Vec<f32>,

    in_gain: LinearSmoother,
    fader: LinearSmoother,
    pan_l: LinearSmoother,
    pan_r: LinearSmoother,
    mute: LinearSmoother,
    sends: [LinearSmoother; NUM_BUSES],

    hpf: Biquad,
    hpf_freq: f32,
    gate: Gate,
    eq_low: Biquad,
    eq_mid: Biquad,
    eq_high: Biquad,
    eq_gains: [f32; 3],
    comp: Compressor,

    meters: StripMeters,
}

impl Strip {
    pub fn new(sr: f64, max_block: usize, ramp: u32) -> Self {
        Strip {
            sr,
            scratch: vec![0.0; max_block.max(1)],
            in_gain: LinearSmoother::new(1.0, ramp),
            fader: LinearSmoother::new(1.0, ramp),
            pan_l: LinearSmoother::new(std::f32::consts::FRAC_1_SQRT_2, ramp),
            pan_r: LinearSmoother::new(std::f32::consts::FRAC_1_SQRT_2, ramp),
            mute: LinearSmoother::new(1.0, ramp),
            sends: [
                LinearSmoother::new(1.0, ramp),
                LinearSmoother::new(1.0, ramp),
                LinearSmoother::new(0.0, ramp),
            ],
            hpf: Biquad::hpf(80.0, sr),
            hpf_freq: 80.0,
            gate: Gate::new(sr),
            eq_low: Biquad::low_shelf(EQ_LOW_HZ, 0.0, sr),
            eq_mid: Biquad::peaking(EQ_MID_HZ, EQ_MID_Q, 0.0, sr),
            eq_high: Biquad::high_shelf(EQ_HIGH_HZ, 0.0, sr),
            eq_gains: [0.0; 3],
            comp: Compressor::new(sr),
            meters: StripMeters::silent(),
        }
    }

    pub fn meters(&self) -> StripMeters {
        self.meters
    }

    /// Update ramp targets from the shared params. `snap` jumps straight to
    /// the targets (first block after construction).
    fn update_targets(&mut self, p: &StripParams, any_solo: bool, snap: bool) {
        let trim = clamp_or(p.trim_db.load(Relaxed), TRIM_MIN_DB, TRIM_MAX_DB, 0.0);
        let sign = if p.polarity_invert.load(Relaxed) { -1.0 } else { 1.0 };
        let in_gain = sign * 10f32.powf(trim / 20.0);

        let pan = clamp_or(p.pan.load(Relaxed), -1.0, 1.0, 0.0);
        let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
        let (pan_l, pan_r) = (angle.cos(), angle.sin());

        let fader = fader_db_to_lin(p.fader_db.load(Relaxed));
        let mute = if p.mute.load(Relaxed) { 0.0 } else { 1.0 };

        let mut sends = [0.0f32; NUM_BUSES];
        for (dst, src) in sends.iter_mut().zip(p.send_db.iter()) {
            *dst = fader_db_to_lin(src.load(Relaxed));
        }
        if any_solo {
            // PFL: only soloed strips reach the Monitor bus, at unity.
            sends[BUS_MONITOR] = if p.solo.load(Relaxed) { 1.0 } else { 0.0 };
        }

        let targets = [
            (&mut self.in_gain, in_gain),
            (&mut self.fader, fader),
            (&mut self.pan_l, pan_l),
            (&mut self.pan_r, pan_r),
            (&mut self.mute, mute),
        ];
        for (smoother, value) in targets {
            if snap {
                smoother.snap(value);
            } else {
                smoother.set_target(value);
            }
        }
        for (smoother, &value) in self.sends.iter_mut().zip(sends.iter()) {
            if snap {
                smoother.snap(value);
            } else {
                smoother.set_target(value);
            }
        }
    }

    /// Recompute filter coefficients only when their settings changed.
    fn update_filters(&mut self, p: &StripParams) {
        let hpf_max = ((self.sr * 0.45) as f32).min(1_000.0).max(20.0);
        let hpf_freq = clamp_or(p.hpf_freq_hz.load(Relaxed), 20.0, hpf_max, 80.0);
        if hpf_freq != self.hpf_freq {
            self.hpf.copy_coeffs_from(&Biquad::hpf(hpf_freq as f64, self.sr));
            self.hpf_freq = hpf_freq;
        }

        let gains = [
            clamp_or(p.eq_low_db.load(Relaxed), -EQ_MAX_DB, EQ_MAX_DB, 0.0),
            clamp_or(p.eq_mid_db.load(Relaxed), -EQ_MAX_DB, EQ_MAX_DB, 0.0),
            clamp_or(p.eq_high_db.load(Relaxed), -EQ_MAX_DB, EQ_MAX_DB, 0.0),
        ];
        if gains[0] != self.eq_gains[0] {
            self.eq_low
                .copy_coeffs_from(&Biquad::low_shelf(EQ_LOW_HZ, gains[0] as f64, self.sr));
        }
        if gains[1] != self.eq_gains[1] {
            self.eq_mid.copy_coeffs_from(&Biquad::peaking(
                EQ_MID_HZ,
                EQ_MID_Q,
                gains[1] as f64,
                self.sr,
            ));
        }
        if gains[2] != self.eq_gains[2] {
            self.eq_high
                .copy_coeffs_from(&Biquad::high_shelf(EQ_HIGH_HZ, gains[2] as f64, self.sr));
        }
        self.eq_gains = gains;

        self.gate
            .set_threshold_db(clamp_or(p.gate_threshold_db.load(Relaxed), -100.0, 0.0, -50.0));
        self.comp
            .set_threshold(clamp_or(p.comp_threshold_db.load(Relaxed), -60.0, 0.0, -18.0));
        self.comp.set_ratio(clamp_or(p.comp_ratio.load(Relaxed), 1.0, 20.0, 3.0));
    }

    /// Process `frames` samples of `input` (zero-extended if shorter) and ADD
    /// the result into the three bus buffers. `frames` must be ≤ max_block.
    pub fn process(
        &mut self,
        input: &[f32],
        frames: usize,
        p: &StripParams,
        any_solo: bool,
        snap: bool,
        buses: &mut [StereoBuffer; NUM_BUSES],
    ) {
        let n = frames.min(self.scratch.len());
        self.update_targets(p, any_solo, snap);
        self.update_filters(p);

        let hpf_on = p.hpf_enabled.load(Relaxed);
        let gate_on = p.gate_enabled.load(Relaxed);
        let comp_on = p.comp_enabled.load(Relaxed);
        let monitor_post = p.monitor_post_fader.load(Relaxed) && !any_solo;

        // Stage 1: gain, HPF, gate, EQ (per sample).
        let mut gate_gain = 0.0f32;
        for i in 0..n {
            let x = input.get(i).copied().unwrap_or(0.0);
            let mut y = x * self.in_gain.next();
            if hpf_on {
                y = self.hpf.tick(y);
            }
            let (delayed, g) = self.gate.tick_raw(y);
            gate_gain = g;
            y = if gate_on { delayed * g } else { delayed };
            y = self.eq_low.tick(y);
            y = self.eq_mid.tick(y);
            y = self.eq_high.tick(y);
            self.scratch[i] = y;
        }

        // Stage 2: compressor (block API).
        let mut gr_db = 0.0f32;
        if comp_on {
            let before = peak(&self.scratch[..n]);
            self.comp.process_block(&mut self.scratch[..n]);
            let after = peak(&self.scratch[..n]);
            if before > 1e-9 {
                gr_db = (20.0 * (after / before).max(1e-9).log10()).min(0.0);
            }
        }

        // Stage 3: fader, pan, sends, mute → buses.
        let mut pre_peak = 0.0f32;
        let mut post_peak = 0.0f32;
        let [main, stream, monitor] = buses;
        for i in 0..n {
            let pre = self.scratch[i];
            let post = pre * self.fader.next();
            let pl = self.pan_l.next();
            let pr = self.pan_r.next();
            let m = self.mute.next();
            let g_main = self.sends[BUS_MAIN].next() * m;
            let g_stream = self.sends[BUS_STREAM].next() * m;
            let g_mon = self.sends[BUS_MONITOR].next() * m;

            main.left[i] += post * g_main * pl;
            main.right[i] += post * g_main * pr;
            stream.left[i] += post * g_stream * pl;
            stream.right[i] += post * g_stream * pr;
            let mon_src = if monitor_post { post } else { pre };
            monitor.left[i] += mon_src * g_mon * pl;
            monitor.right[i] += mon_src * g_mon * pr;

            pre_peak = pre_peak.max(pre.abs());
            post_peak = post_peak.max(post.abs());
        }

        self.meters = StripMeters {
            pre_fader_peak_db: lin_to_db(pre_peak),
            post_fader_peak_db: lin_to_db(post_peak),
            gate_open: !gate_on || gate_gain > 0.5,
            gain_reduction_db: gr_db,
        };
    }
}

fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0f32, |m, &s| m.max(s.abs()))
}

pub(super) fn lin_to_db(lin: f32) -> f32 {
    if lin > 1e-6 {
        20.0 * lin.log10()
    } else {
        -120.0
    }
}

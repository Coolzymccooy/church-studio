//! A strip's channel processing up to the pre-fader point: trim × polarity
//! (ramped) → HPF → gate (switched) → 3-band EQ → compressor (crossfaded on
//! and off). Split out of `Strip` so the voice strip can run it twice: once
//! on its live (fast) signal for Main and the monitors, once on its full
//! voice-chain signal for Stream. Allocation-free per block.
use super::params::{clamp_or, StripParams, EQ_MAX_DB};
use super::smooth::LinearSmoother;
use crate::dsp::biquad::Biquad;
use crate::dsp::compressor::Compressor;
use crate::dsp::gate::SwitchedGate;
use std::sync::atomic::Ordering::Relaxed;

const EQ_LOW_HZ: f64 = 100.0;
const EQ_MID_HZ: f64 = 1_000.0;
const EQ_MID_Q: f64 = 0.9;
const EQ_HIGH_HZ: f64 = 8_000.0;

pub struct ChannelProc {
    sr: f64,
    pub(super) scratch: Vec<f32>,
    comp_buf: Vec<f32>,
    pub(super) in_gain: LinearSmoother,
    pub(super) comp_mix: LinearSmoother,
    hpf: Biquad,
    hpf_freq: f32,
    gate: SwitchedGate,
    eq_low: Biquad,
    eq_mid: Biquad,
    eq_high: Biquad,
    eq_gains: [f32; 3],
    comp: Compressor,
}

/// What one run of the channel processing reports for the meters.
pub struct ChannelReadout {
    pub gate_gain: f32,
    pub gr_db: f32,
}

impl ChannelProc {
    pub fn new(sr: f64, max_block: usize, ramp: u32) -> Self {
        ChannelProc {
            sr,
            scratch: vec![0.0; max_block.max(1)],
            comp_buf: vec![0.0; max_block.max(1)],
            in_gain: LinearSmoother::new(1.0, ramp),
            comp_mix: LinearSmoother::new(0.0, ramp),
            hpf: Biquad::hpf(80.0, sr),
            hpf_freq: 80.0,
            gate: {
                let mut gate = SwitchedGate::new(sr);
                gate.enabled = false; // synced from the params every block
                gate
            },
            eq_low: Biquad::low_shelf(EQ_LOW_HZ, 0.0, sr),
            eq_mid: Biquad::peaking(EQ_MID_HZ, EQ_MID_Q, 0.0, sr),
            eq_high: Biquad::high_shelf(EQ_HIGH_HZ, 0.0, sr),
            eq_gains: [0.0; 3],
            comp: Compressor::new(sr),
        }
    }

    /// Gate lookahead while the gate is on, otherwise 0.
    pub fn latency_samples(&self) -> usize {
        self.gate.latency_samples()
    }

    pub fn set_targets(&mut self, in_gain: f32, comp_mix: f32, snap: bool) {
        for (smoother, value) in [(&mut self.in_gain, in_gain), (&mut self.comp_mix, comp_mix)] {
            if snap {
                smoother.snap(value);
            } else {
                smoother.set_target(value);
            }
        }
    }

    /// Recompute filter coefficients only when their settings changed.
    pub fn update_filters(&mut self, p: &StripParams) {
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
            self.eq_low.copy_coeffs_from(&Biquad::low_shelf(EQ_LOW_HZ, gains[0] as f64, self.sr));
        }
        if gains[1] != self.eq_gains[1] {
            self.eq_mid
                .copy_coeffs_from(&Biquad::peaking(EQ_MID_HZ, EQ_MID_Q, gains[1] as f64, self.sr));
        }
        if gains[2] != self.eq_gains[2] {
            self.eq_high.copy_coeffs_from(&Biquad::high_shelf(EQ_HIGH_HZ, gains[2] as f64, self.sr));
        }
        self.eq_gains = gains;
        self.gate
            .set_threshold_db(clamp_or(p.gate_threshold_db.load(Relaxed), -100.0, 0.0, -50.0));
        self.comp
            .set_threshold(clamp_or(p.comp_threshold_db.load(Relaxed), -60.0, 0.0, -18.0));
        self.comp.set_ratio(clamp_or(p.comp_ratio.load(Relaxed), 1.0, 20.0, 3.0));
    }

    /// Process `n` samples of `input` (zero-extended) into `scratch`.
    /// `det` receives the post-HPF signal for the automix detector, if given.
    pub fn run(
        &mut self,
        input: &[f32],
        n: usize,
        hpf_on: bool,
        gate_on: bool,
        mut det: Option<&mut [f32]>,
    ) -> ChannelReadout {
        for i in 0..n {
            let x = input.get(i).copied().unwrap_or(0.0);
            let mut y = x * self.in_gain.next();
            if hpf_on {
                y = self.hpf.tick(y);
            }
            self.scratch[i] = y;
            if let Some(det) = det.as_deref_mut() {
                det[i] = y;
            }
        }

        // Gate: zero latency and no work while off.
        self.gate.enabled = gate_on;
        let gate_gain = self.gate.process_block(&mut self.scratch[..n]);

        for y in self.scratch[..n].iter_mut() {
            let mut v = self.eq_low.tick(*y);
            v = self.eq_mid.tick(v);
            *y = self.eq_high.tick(v);
        }

        // Compressor, crossfaded on/off over the ramp so switching it never
        // clicks. Skipped entirely while fully off.
        let mut gr_db = 0.0f32;
        let comp_active = self.comp_mix.current() > 0.0 || self.comp_mix.target() > 0.0;
        if comp_active {
            self.comp_buf[..n].copy_from_slice(&self.scratch[..n]);
            self.comp.process_block(&mut self.comp_buf[..n]);
            let before = peak(&self.scratch[..n]);
            let after = peak(&self.comp_buf[..n]);
            if before > 1e-9 && self.comp_mix.target() > 0.0 {
                gr_db = (20.0 * (after / before).max(1e-9).log10()).min(0.0);
            }
            for i in 0..n {
                let mix = self.comp_mix.next();
                let dry = self.scratch[i];
                self.scratch[i] = dry + (self.comp_buf[i] - dry) * mix;
            }
        }
        ChannelReadout { gate_gain, gr_db }
    }
}

fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

//! One mono input channel strip.
//!
//! Signal flow:
//!   input → trim × polarity (ramped) → HPF → gate (switched, lookahead)
//!         → 3-band EQ → compressor → [pre-fader] → fader (ramped)
//!         × automix × duck (per-sample, from `automix.rs`) → [post-fader]
//!         → pan (constant power, 0 dB at centre, ramped) × send (ramped)
//!         × mute (ramped) → buses
//!
//! The strip runs in two passes so the automixer can see every strip at the
//! same sample: `process_pre` (up to the pre-fader point, plus a copy of the
//! post-HPF signal for the detector) and `process_out` (fader × automix ×
//! duck, pan, sends). Automix and duck reach the post-fader paths only; the
//! pre-fader Monitor sends and PFL stay untouched.
//!
//! Main and Stream sends are post-fader. The Monitor (mon 1) send is
//! pre-fader unless `monitor_post_fader` is set. The Monitor 2 and Monitor 3
//! sends (mon 2, mon 3) are always pre-fader: those mixes never follow the
//! front-of-house fader. Mute removes the strip from every bus mix.
//!
//! Solo is PFL (live-console standard): when any strip is soloed the Monitor
//! (mon 1) bus carries ONLY the soloed strips, pre-fader, at unity (0 dB) on BOTH
//! sides — no pan law, no send level — and PFL IGNORES mute, so the operator
//! can check a muted mic before opening it. Main, Stream, mon 2 and mon 3
//! never see solo.
//!
//! Latency: the gate is a `SwitchedGate`, the same live-safe switch as the
//! voice chain. Off, it is a zero-latency passthrough; switched on, it plays
//! dry while its 20 ms lookahead fills and then crossfades to the gated
//! signal. So a strip only carries the 20 ms while its gate is on (the
//! strip's latency is `latency_samples()`). Trade-off: a gated strip is
//! 20 ms behind an ungated one, which matters only when two strips carry the
//! same source.
use super::channel_proc::ChannelProc;
use super::fx_params::FX_COUNT;
use super::params::{
    clamp_or, fader_db_to_lin, StripParams, BUS_MAIN, BUS_MONITOR, BUS_MONITOR2, BUS_MONITOR3,
    BUS_STREAM, FADER_MAX_DB, NUM_BUSES, PAN_LAW_NORM, TRIM_MAX_DB, TRIM_MIN_DB,
};
use super::groups::StripGroupGain;
use super::smooth::LinearSmoother;
use super::StereoBuffer;
use std::sync::atomic::Ordering::Relaxed;

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
    /// Automix attenuation, dB (≤ 0; deepest of the block).
    pub automix_gr_db: f32,
    /// Auto-duck attenuation, dB (≤ 0; deepest of the block).
    pub duck_gr_db: f32,
    /// The automix detector hears speech on this strip.
    pub speech: bool,
}

impl StripMeters {
    fn silent() -> Self {
        StripMeters {
            pre_fader_peak_db: -120.0,
            post_fader_peak_db: -120.0,
            gate_open: false,
            gain_reduction_db: 0.0,
            automix_gr_db: 0.0,
            duck_gr_db: 0.0,
            speech: false,
        }
    }
}

pub struct Strip {
    /// Channel processing of the strip's input (what every bus hears).
    proc: ChannelProc,
    /// Channel processing of the Stream-only input (the voice strip's full
    /// voice-chain signal). Used only in blocks that pass one.
    stream_proc: ChannelProc,
    stream_alt: bool,
    /// Post-trim/HPF copy of the block for the automix detector.
    det_buf: Vec<f32>,
    /// Frames, gate state and compressor GR from `process_pre`.
    pending_frames: usize,
    pending_gate_open: bool,
    pending_gr_db: f32,
    monitor_post: bool,

    fader: LinearSmoother,
    group: StripGroupGain,
    pan_l: LinearSmoother,
    pan_r: LinearSmoother,
    mute: LinearSmoother,
    sends: [LinearSmoother; NUM_BUSES],
    fx_sends: [LinearSmoother; FX_COUNT],
    /// PFL gain (1 while this strip is soloed, else 0). Not affected by mute.
    pfl: LinearSmoother,

    meters: StripMeters,
}

impl Strip {
    pub fn new(sr: f64, max_block: usize, ramp: u32) -> Self {
        Strip {
            proc: ChannelProc::new(sr, max_block, ramp),
            stream_proc: ChannelProc::new(sr, max_block, ramp),
            stream_alt: false,
            det_buf: vec![0.0; max_block.max(1)],
            pending_frames: 0,
            pending_gate_open: true,
            pending_gr_db: 0.0,
            monitor_post: false,
            fader: LinearSmoother::new(1.0, ramp),
            group: StripGroupGain::default(),
            pan_l: LinearSmoother::new(1.0, ramp),
            pan_r: LinearSmoother::new(1.0, ramp),
            mute: LinearSmoother::new(1.0, ramp),
            sends: [
                LinearSmoother::new(1.0, ramp),
                LinearSmoother::new(1.0, ramp),
                LinearSmoother::new(0.0, ramp),
                LinearSmoother::new(0.0, ramp),
                LinearSmoother::new(0.0, ramp),
            ],
            fx_sends: std::array::from_fn(|_| LinearSmoother::new(0.0, ramp)),
            pfl: LinearSmoother::new(0.0, ramp),
            meters: StripMeters::silent(),
        }
    }

    pub(super) fn set_group_gain(&mut self, group: StripGroupGain) {
        self.group = group;
    }

    pub fn meters(&self) -> StripMeters {
        self.meters
    }

    /// Current processing latency in samples: the gate lookahead while the
    /// gate is on, otherwise 0.
    pub fn latency_samples(&self) -> usize {
        self.proc.latency_samples()
    }

    /// Update ramp targets from the shared params. `snap` jumps straight to
    /// the targets (first block after construction).
    fn update_targets(&mut self, p: &StripParams, any_solo: bool, snap: bool) {
        let trim = clamp_or(p.trim_db.load(Relaxed), TRIM_MIN_DB, TRIM_MAX_DB, 0.0);
        let sign = if p.polarity_invert.load(Relaxed) { -1.0 } else { 1.0 };
        let in_gain = sign * 10f32.powf(trim / 20.0);

        let pan = clamp_or(p.pan.load(Relaxed), -1.0, 1.0, 0.0);
        let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
        let (pan_l, pan_r) = (angle.cos() * PAN_LAW_NORM, angle.sin() * PAN_LAW_NORM);

        let fader = (fader_db_to_lin(p.fader_db.load(Relaxed)) * self.group.level).min(fader_db_to_lin(FADER_MAX_DB));
        let mute = if p.mute.load(Relaxed) || self.group.muted { 0.0 } else { 1.0 };

        let mut sends = [0.0f32; NUM_BUSES];
        for (dst, src) in sends.iter_mut().zip(p.send_db.iter()) {
            *dst = fader_db_to_lin(src.load(Relaxed));
        }
        // While anything is soloed the Monitor (mon 1) bus is PFL only: the
        // normal monitor send is silenced and soloed strips come in via
        // `pfl`. Mon 2 and mon 3 keep their sends.
        let pfl = if any_solo && p.solo.load(Relaxed) { 1.0 } else { 0.0 };
        let comp_mix = if p.comp_enabled.load(Relaxed) { 1.0 } else { 0.0 };
        if any_solo {
            sends[BUS_MONITOR] = 0.0;
        }

        self.proc.set_targets(in_gain, comp_mix, snap);
        self.stream_proc.set_targets(in_gain, comp_mix, snap);
        let targets = [
            (&mut self.fader, fader),
            (&mut self.pan_l, pan_l),
            (&mut self.pan_r, pan_r),
            (&mut self.mute, mute),
            (&mut self.pfl, pfl),
        ];
        for (smoother, value) in targets {
            if snap {
                smoother.snap(value);
            } else {
                smoother.set_target(value);
            }
        }
        let fx_sends = p.fx_send_db.iter().map(|param| fader_db_to_lin(param.load(Relaxed)));
        for (smoother, value) in self.sends.iter_mut().chain(&mut self.fx_sends).zip(sends.into_iter().chain(fx_sends)) {
            if snap {
                smoother.snap(value);
            } else {
                smoother.set_target(value);
            }
        }
    }

    /// Recompute filter coefficients only when their settings changed.
    fn update_filters(&mut self, p: &StripParams) {
        self.proc.update_filters(p);
        self.stream_proc.update_filters(p);
    }

    /// Both passes at unity automix gain (a strip on its own).
    pub fn process(
        &mut self,
        input: &[f32],
        frames: usize,
        p: &StripParams,
        any_solo: bool,
        snap: bool,
        buses: &mut [StereoBuffer; NUM_BUSES],
    ) {
        self.process_pre(input, frames, p, any_solo, snap);
        self.process_out(&[], buses, None);
    }

    /// Detection signal of the last `process_pre` (post trim and HPF).
    pub fn detection(&self) -> &[f32] {
        &self.det_buf[..self.pending_frames]
    }

    /// Pass 1: process `frames` samples of `input` (zero-extended if shorter)
    /// up to the pre-fader point. `frames` must be ≤ max_block.
    pub fn process_pre(
        &mut self,
        input: &[f32],
        frames: usize,
        p: &StripParams,
        any_solo: bool,
        snap: bool,
    ) {
        self.process_pre_split(input, None, frames, p, any_solo, snap);
    }

    /// `process_pre` with an optional Stream-only input. The voice strip
    /// passes its live (fast) signal as `input`, which Main, the monitors
    /// and the automix detector use, and its full voice-chain signal as
    /// `stream_input`, which only the Stream send uses. Both go through the
    /// same strip settings (trim, HPF, gate, EQ, compressor).
    pub fn process_pre_split(
        &mut self,
        input: &[f32],
        stream_input: Option<&[f32]>,
        frames: usize,
        p: &StripParams,
        any_solo: bool,
        snap: bool,
    ) {
        let n = frames.min(self.det_buf.len());
        self.update_targets(p, any_solo, snap);
        self.update_filters(p);

        let hpf_on = p.hpf_enabled.load(Relaxed);
        let gate_on = p.gate_enabled.load(Relaxed);
        let monitor_post = p.monitor_post_fader.load(Relaxed) && !any_solo;

        let readout = self.proc.run(input, n, hpf_on, gate_on, Some(&mut self.det_buf[..n]));
        self.stream_alt = stream_input.is_some();
        if let Some(alt) = stream_input {
            self.stream_proc.run(alt, n, hpf_on, gate_on, None);
        }

        self.pending_frames = n;
        self.pending_gate_open = !gate_on || readout.gate_gain > 0.5;
        self.pending_gr_db = readout.gr_db;
        self.monitor_post = monitor_post;
    }

    /// Pass 2: fader × `auto_gain[i]` (automix × duck; missing samples are
    /// unity), pan, sends and mute, ADDED into the bus buffers.
    pub fn process_out(
        &mut self,
        auto_gain: &[f32],
        buses: &mut [StereoBuffer; NUM_BUSES],
        mut fx: Option<&mut [StereoBuffer; FX_COUNT]>,
    ) {
        let n = self.pending_frames;
        let monitor_post = self.monitor_post;
        // Stage 3: fader, pan, sends, mute → buses.
        let mut pre_peak = 0.0f32;
        let mut post_peak = 0.0f32;
        let [main, stream, monitor, monitor2, monitor3] = buses;
        if self.fx_sends.iter().all(|send| send.current() == 0.0 && send.target() == 0.0) {
            fx = None;
        }
        let stream_alt = self.stream_alt;
        for i in 0..n {
            let pre = self.proc.scratch[i];
            let auto = auto_gain.get(i).copied().unwrap_or(1.0);
            let fader = self.fader.next();
            let post = pre * fader * auto;
            // Stream hears the full voice chain when the strip carries one.
            let stream_post = if stream_alt { self.stream_proc.scratch[i] * fader * auto } else { post };
            let pl = self.pan_l.next();
            let pr = self.pan_r.next();
            let m = self.mute.next();
            let g_main = self.sends[BUS_MAIN].next() * m;
            let g_stream = self.sends[BUS_STREAM].next() * m;
            let g_mon = self.sends[BUS_MONITOR].next() * m;
            let g_mon2 = self.sends[BUS_MONITOR2].next() * m;
            let g_mon3 = self.sends[BUS_MONITOR3].next() * m;

            main.left[i] += post * g_main * pl;
            main.right[i] += post * g_main * pr;
            stream.left[i] += stream_post * g_stream * pl;
            stream.right[i] += stream_post * g_stream * pr;
            let pfl = self.pfl.next(); // unity, both sides, ignores mute
            let mon_src = if monitor_post { post } else { pre };
            monitor.left[i] += mon_src * g_mon * pl + pre * pfl;
            monitor.right[i] += mon_src * g_mon * pr + pre * pfl;
            // Mon 2 / mon 3: always pre-fader, no PFL.
            monitor2.left[i] += pre * g_mon2 * pl;
            monitor2.right[i] += pre * g_mon2 * pr;
            monitor3.left[i] += pre * g_mon3 * pl;
            monitor3.right[i] += pre * g_mon3 * pr;

            if let Some(returns) = fx.as_mut() {
                for (buf, send) in returns.iter_mut().zip(&mut self.fx_sends) {
                    let wet = post * m * send.next();
                    buf.left[i] += wet * pl;
                    buf.right[i] += wet * pr;
                }
            }

            pre_peak = pre_peak.max(pre.abs());
            post_peak = post_peak.max(post.abs());
        }

        self.meters = StripMeters {
            pre_fader_peak_db: lin_to_db(pre_peak),
            post_fader_peak_db: lin_to_db(post_peak),
            gate_open: self.pending_gate_open,
            gain_reduction_db: self.pending_gr_db,
            automix_gr_db: 0.0,
            duck_gr_db: 0.0,
            speech: false,
        };
    }

    /// Attach the automix readouts to this block's meters.
    pub fn set_auto_meters(&mut self, automix_gr_db: f32, duck_gr_db: f32, speech: bool) {
        self.meters.automix_gr_db = automix_gr_db;
        self.meters.duck_gr_db = duck_gr_db;
        self.meters.speech = speech;
    }
}


pub(super) fn lin_to_db(lin: f32) -> f32 {
    if lin > 1e-6 {
        20.0 * lin.log10()
    } else {
        -120.0
    }
}

use super::fx::{Delay, Reverb};
use super::fx_params::{FxParams, ReturnParams, FX_COUNT};
use super::params::{fader_db_to_lin, BUS_MAIN, BUS_MONITOR, BUS_STREAM, NUM_BUSES};
use super::smooth::LinearSmoother;
use super::strip::lin_to_db;
use super::StereoBuffer;
use std::sync::atomic::Ordering::Relaxed;

struct Return {
    fader: LinearSmoother,
    gain: LinearSmoother,
    sends: [LinearSmoother; NUM_BUSES],
    pfl: LinearSmoother,
    monitor_post: LinearSmoother,
    peak: [f32; 2],
}

impl Return {
    fn new(ramp: u32) -> Self {
        Self {
            fader: LinearSmoother::new(0.0, ramp),
            gain: LinearSmoother::new(0.0, ramp),
            sends: std::array::from_fn(|_| LinearSmoother::new(0.0, ramp)),
            pfl: LinearSmoother::new(0.0, ramp),
            monitor_post: LinearSmoother::new(0.0, ramp),
            peak: [0.0; 2],
        }
    }

    fn prepare(&mut self, params: &ReturnParams, any_solo: bool, snap: bool) {
        self.peak = [0.0; 2];
        let update = |smooth: &mut LinearSmoother, value| {
            if snap {
                smooth.snap(value);
            } else {
                smooth.set_target(value);
            }
        };
        update(&mut self.fader, fader_db_to_lin(params.fader.load(Relaxed)));
        update(&mut self.gain, f32::from(!params.mute.load(Relaxed)));
        update(
            &mut self.monitor_post,
            f32::from(params.monitor_post.load(Relaxed)),
        );
        update(
            &mut self.pfl,
            f32::from(any_solo && params.solo.load(Relaxed)),
        );

        for (i, (smooth, param)) in self.sends.iter_mut().zip(&params.sends).enumerate() {
            let level = if any_solo && i == BUS_MONITOR {
                0.0
            } else {
                fader_db_to_lin(param.load(Relaxed))
            };
            update(smooth, level);
        }
    }

    fn route(&mut self, i: usize, wet: [f32; 2], buses: &mut [StereoBuffer; NUM_BUSES]) {
        let raw_fader = self.fader.next();
        let gain = self.gain.next();
        let post = raw_fader * gain;
        let pfl = self.pfl.next();
        let monitor = gain + (post - gain) * self.monitor_post.next();
        self.peak[0] = self.peak[0].max((wet[0] * post).abs());
        self.peak[1] = self.peak[1].max((wet[1] * post).abs());

        for (b, (bus, send)) in buses.iter_mut().zip(&mut self.sends).enumerate() {
            let source = match b {
                BUS_MAIN | BUS_STREAM => post,
                BUS_MONITOR => monitor,
                _ => gain,
            };
            let level = source * send.next() + if b == BUS_MONITOR { pfl } else { 0.0 };
            bus.left[i] += wet[0] * level;
            bus.right[i] += wet[1] * level;
        }
    }
}

pub(super) struct FxRack {
    pub(super) sends: [StereoBuffer; FX_COUNT],
    reverb: Reverb,
    delay: Delay,
    returns: [Return; FX_COUNT],
}

impl FxRack {
    pub(super) fn new(sr: f32, max_block: usize, ramp: u32) -> Self {
        Self {
            sends: std::array::from_fn(|_| StereoBuffer::new(max_block)),
            reverb: Reverb::new(sr),
            delay: Delay::new(sr),
            returns: std::array::from_fn(|_| Return::new(ramp)),
        }
    }

    pub(super) fn clear(&mut self, frames: usize) {
        for buf in &mut self.sends {
            buf.clear(frames);
        }
    }

    pub(super) fn process(
        &mut self,
        buses: &mut [StereoBuffer; NUM_BUSES],
        frames: usize,
        params: &FxParams,
        any_solo: bool,
        snap: bool,
    ) {
        self.reverb.set_decay(params.decay.load(Relaxed), snap);
        self.delay.configure(
            params.tempo.load(Relaxed),
            params.feedback.load(Relaxed),
            snap,
        );
        for (ret, params) in self.returns.iter_mut().zip(&params.returns) {
            ret.prepare(params, any_solo, snap);
        }
        let [a, b] = &self.sends;
        let [reverb_return, delay_return] = &mut self.returns;
        for i in 0..frames {
            reverb_return.route(i, self.reverb.tick([a.left[i], a.right[i]]), buses);
            delay_return.route(i, self.delay.tick([b.left[i], b.right[i]]), buses);
        }
    }

    pub(super) fn peaks(&self) -> [[f32; 2]; FX_COUNT] {
        std::array::from_fn(|i| self.returns[i].peak.map(lin_to_db))
    }
}

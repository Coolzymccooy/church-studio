//! Hyper gate decision: native port of the worklet's tiered voice detection
//! (`public/worklets/hypergate-worklet.js`, `process`).
//!
//! Each analysis frame adds up a voice score from the frame features, the
//! mic fingerprint, the syllable-rate modulation of the voice band and (when
//! the RNNoise stage runs) its voice-activity probability. The score sets a
//! 350 ms voice hold and a 250 ms gate hold; per sample those holds and the
//! frame level give the gain target, exactly as in the worklet:
//! - Balanced: anything above the threshold that is not clearly noise opens
//!   the gate; quiet speech that passed the voice check is lifted.
//! - Speech only: the gate opens only for confirmed speech. Everything else
//!   is capped at `SPEECH_CAP` (−26 dB). Speech must also be syllabic (the
//!   voice band dips between syllables) and, when RNNoise runs, voiced.
//!
//! Hold timers count samples and frames close on fixed sample counts, so the
//! result does not depend on the block size. No allocation after `new`.
use super::analysis::{FrameAnalyser, FrameFeatures};
use super::fingerprint::{Fingerprint, LearnOutcome, Learner};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HyperMode {
    Balanced,
    Speech,
}

/// Closed gain (−36.5 dB), as in the worklet.
pub const FLOOR_GAIN: f32 = 0.015;
/// Speech-only ceiling for anything that is not speech (−26 dB).
pub const SPEECH_CAP: f32 = 0.05;

const VOICE_HOLD_MS: f64 = 350.0;
const GATE_HOLD_MS: f64 = 250.0;
const OPEN_ABOVE_DB: f32 = 3.0;
const CLOSE_BELOW_DB: f32 = 4.0;
const LOUD_ABOVE_DB: f32 = 15.0;

/// Voice-band level history used for the modulation depth.
const MOD_WINDOW_MS: f64 = 450.0;
const MOD_FLOOR_DB: f32 = -90.0;
/// Speech dips by 25–60 dB between syllables; sustained music by 5–16 dB
/// (most at chord changes), steady noise by about 5 dB.
const SYLLABIC_DB: f32 = 20.0;
const STEADY_DB: f32 = 8.0;

/// White/pink noise reads ~0.85; voiced speech 0.1–0.3, fricatives ≤ 0.65.
const FLATNESS_NOISE: f32 = 0.6;
const ZCR_HISS: f32 = 0.35;
const VOICE_SHARE: f32 = 0.5;
const VAD_SPEECH: f32 = 0.5;
/// Voiced frames only for "learn mic": vowels, not fricatives.
const LEARN_FLATNESS: f32 = 0.45;
const LEARN_ZCR: f32 = 0.2;
/// Speech only, once a mic is learned: when the mean match of the last
/// `FP_RECENT` active frames is under this, the sound is not this mic's
/// talker (a TV, a distant voice). Averaging stops single frames that happen
/// to match from opening the gate; it costs ~3 frames (32 ms) when the
/// talker takes over from such a source.
const FP_REJECT: f32 = 0.55;
const FP_RECENT: usize = 6;

const BALANCED_SCORE: i32 = 4;
const SPEECH_SCORE: i32 = 8;

/// "Learn mic" listens for 5 s and needs at least 1 s of voice.
const LEARN_WINDOW_S: f64 = 5.0;
const LEARN_MIN_VOICE_S: f64 = 1.0;

/// What one frame says about the audio at the lookahead head.
#[derive(Clone, Copy, Debug, Default)]
pub struct Verdict {
    pub syllabic: bool,
    pub voice: bool,
    pub can_open: bool,
    pub loud_override: bool,
}

pub struct HyperDetector {
    analyser: FrameAnalyser,
    mod_ring: Vec<f32>,
    mod_pos: usize,
    vad_peak: f32,
    vad_seen: bool,
    fingerprint: Option<Fingerprint>,
    /// Match scores of the last `FP_RECENT` frames above the threshold.
    fp_recent: [f32; FP_RECENT],
    fp_pos: usize,
    learner: Learner,
    learn_window: usize,
    learn_min_frames: usize,
    voice_hold_len: usize,
    gate_hold_len: usize,
    voice_hold: usize,
    gate_hold: usize,
    level_db: f32,
    last: Verdict,
    pub mode: HyperMode,
    pub threshold_db: f32,
}

fn samples(ms: f64, sr: f64) -> usize {
    (sr * ms / 1000.0).round() as usize
}

impl HyperDetector {
    pub fn new(sr: f64) -> Self {
        let analyser = FrameAnalyser::new(sr);
        let frame = analyser.frame_len();
        let mod_frames = (samples(MOD_WINDOW_MS, sr) / frame).max(2);
        HyperDetector {
            analyser,
            mod_ring: vec![MOD_FLOOR_DB; mod_frames],
            mod_pos: 0,
            vad_peak: 0.0,
            vad_seen: false,
            fingerprint: None,
            fp_recent: [1.0; FP_RECENT],
            fp_pos: 0,
            learner: Learner::new(),
            learn_window: (sr * LEARN_WINDOW_S) as usize,
            learn_min_frames: ((sr * LEARN_MIN_VOICE_S) as usize / frame).max(2),
            voice_hold_len: samples(VOICE_HOLD_MS, sr),
            gate_hold_len: samples(GATE_HOLD_MS, sr),
            voice_hold: 0,
            gate_hold: 0,
            level_db: -120.0,
            last: Verdict::default(),
            mode: HyperMode::Balanced,
            threshold_db: -45.0,
        }
    }

    /// Clear the analysis and holds (gate switched on). Keeps the
    /// fingerprint and any learning in progress.
    pub fn reset(&mut self) {
        self.analyser.reset();
        self.mod_ring.fill(MOD_FLOOR_DB);
        self.mod_pos = 0;
        self.vad_peak = 0.0;
        self.vad_seen = false;
        self.voice_hold = 0;
        self.gate_hold = 0;
        self.level_db = -120.0;
        self.last = Verdict::default();
        self.fp_recent = [1.0; FP_RECENT];
    }

    pub fn set_fingerprint(&mut self, fp: Option<Fingerprint>) {
        self.fingerprint = fp;
        self.fp_recent = [1.0; FP_RECENT];
    }

    #[cfg(test)]
    pub fn fingerprint(&self) -> Option<&Fingerprint> {
        self.fingerprint.as_ref()
    }

    pub fn start_learning(&mut self) {
        self.learner.start(self.learn_window, self.learn_min_frames);
    }

    pub fn is_learning(&self) -> bool {
        self.learner.is_active()
    }

    pub fn cancel_learning(&mut self) {
        self.learner.cancel();
    }

    pub fn take_learn_outcome(&mut self) -> Option<LearnOutcome> {
        self.learner.take_outcome()
    }

    /// Feed one sample (the lookahead head) and the current RNNoise VAD
    /// (None when that stage is off). Returns the gain target (0..1).
    #[inline]
    pub fn tick(&mut self, x: f32, vad: Option<f32>) -> f32 {
        if let Some(v) = vad.filter(|v| v.is_finite()) {
            self.vad_peak = self.vad_peak.max(v);
            self.vad_seen = true;
        }
        if let Some(f) = self.analyser.push(x) {
            let f = *f;
            self.on_frame(&f);
        }
        let target = self.target();
        self.voice_hold = self.voice_hold.saturating_sub(1);
        self.gate_hold = self.gate_hold.saturating_sub(1);
        target
    }

    fn target(&self) -> f32 {
        let thr = self.threshold_db;
        let open = thr + OPEN_ABOVE_DB;
        let close = thr - CLOSE_BELOW_DB;
        let db = self.level_db;
        let voice_active = self.voice_hold > 0;

        let mut target = if voice_active || self.gate_hold > 0 {
            1.0
        } else if db < close {
            FLOOR_GAIN
        } else {
            let progress = ((db - close) / (open - close)).clamp(0.0, 1.0);
            FLOOR_GAIN + progress * (1.0 - FLOOR_GAIN)
        };
        if self.mode == HyperMode::Speech && !voice_active && !self.last.loud_override {
            target = target.min(SPEECH_CAP);
        }
        if voice_active && db < thr {
            let lift = (0.25 + (thr - db) * 0.025).min(0.5);
            target = target.max(lift);
        }
        target
    }

    fn on_frame(&mut self, f: &FrameFeatures) {
        self.mod_ring[self.mod_pos] = f.voice_db.max(MOD_FLOOR_DB);
        self.mod_pos = (self.mod_pos + 1) % self.mod_ring.len();
        let depth = modulation_depth(&self.mod_ring);

        let vad = if self.vad_seen { Some(self.vad_peak) } else { None };
        self.vad_peak = 0.0;
        self.vad_seen = false;

        let fp_match = self.fingerprint.as_ref().map(|fp| fp.match_score(f));
        if let Some(m) = fp_match.filter(|_| f.level_db > self.threshold_db) {
            self.fp_recent[self.fp_pos] = m;
            self.fp_pos = (self.fp_pos + 1) % FP_RECENT;
        }
        let fp_recent = fp_match.map(|_| self.fp_recent.iter().sum::<f32>() / FP_RECENT as f32);
        let verdict = verdict(f, self.mode, self.threshold_db, depth, vad, fp_match, fp_recent);

        let frame_len = self.analyser.frame_len();
        let learn_voice = f.level_db > self.threshold_db
            && !f.high_only_noise
            && f.flatness < LEARN_FLATNESS
            && f.zcr < LEARN_ZCR
            && verdict.syllabic
            && vad.map_or(true, |v| v >= VAD_SPEECH);
        self.learner.frame(f, learn_voice, frame_len);

        if verdict.voice {
            self.voice_hold = self.voice_hold_len;
        }
        if verdict.can_open {
            self.gate_hold = self.gate_hold_len;
        }
        self.level_db = f.level_db;
        self.last = verdict;
    }
}

/// Max − min of the voice-band level over the history window, dB.
fn modulation_depth(ring: &[f32]) -> f32 {
    let (lo, hi) = ring
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    hi - lo
}

/// Score one frame (the worklet's tiers plus modulation and VAD) and decide.
fn verdict(
    f: &FrameFeatures,
    mode: HyperMode,
    thr: f32,
    depth: f32,
    vad: Option<f32>,
    fp_match: Option<f32>,
    fp_recent: Option<f32>,
) -> Verdict {
    let open = thr + OPEN_ABOVE_DB;
    let not_noise = f.flatness < FLATNESS_NOISE;
    let has_voice_energy = f.voice_share >= VOICE_SHARE && f.voice_ratio_db > -6.0;
    let syllabic = depth >= SYLLABIC_DB;
    let vad_ok = vad.map_or(true, |v| v >= VAD_SPEECH);

    let mut score = 0;
    score += (f.level_db > thr) as i32 + (f.level_db > open) as i32;
    score += not_noise as i32;
    score += (f.zcr < ZCR_HISS) as i32;
    score += 2 * (f.formant_score > 0.0) as i32;
    score += 2 * has_voice_energy as i32;
    score += (f.voice_ratio_db > 0.0) as i32;
    score += (!f.high_only_noise) as i32;
    score += match fp_match {
        None => 1, // neutral until learned, as in the worklet
        Some(m) if m > 0.7 => 2,
        Some(m) if m > 0.4 => 1,
        Some(m) if m < 0.3 => -2,
        Some(_) => 0,
    };
    score += if syllabic { 2 } else if depth < STEADY_DB { -2 } else { 0 };
    score += match vad {
        None => 0,
        Some(v) if v >= 0.8 => 2,
        Some(v) if v >= VAD_SPEECH => 1,
        Some(v) if v < 0.2 => -1,
        Some(_) => 0,
    };

    let basic_voice = !f.high_only_noise && f.level_db > thr;
    let loud = f.level_db > thr + LOUD_ABOVE_DB && !f.high_only_noise && f.voice_ratio_db > 0.0;

    match mode {
        HyperMode::Balanced => {
            let voice = loud || (basic_voice && score >= BALANCED_SCORE);
            let can_open = loud || (f.level_db > open && score >= 3);
            Verdict { syllabic, voice, can_open, loud_override: loud }
        }
        HyperMode::Speech => {
            // Speech must dip between syllables, be voiced when RNNoise runs
            // and, once the mic is learned, sound like that mic.
            let fp_ok = fp_recent.map_or(true, |m| m >= FP_REJECT);
            let speechlike = syllabic && vad_ok && not_noise && fp_ok;
            let loud = loud && speechlike;
            let voice = loud
                || (speechlike
                    && basic_voice
                    && score >= SPEECH_SCORE
                    && (has_voice_energy || score >= SPEECH_SCORE + 1));
            let can_open = loud || (speechlike && f.level_db > open && score >= SPEECH_SCORE - 1);
            Verdict { syllabic, voice, can_open, loud_override: loud }
        }
    }
}

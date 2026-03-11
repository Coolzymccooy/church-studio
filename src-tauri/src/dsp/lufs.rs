/// ITU-R BS.1770-4 LUFS metering.
/// K-weighting = Stage 1 high-shelf pre-filter (+4dB @ 1682Hz) +
///               Stage 2 RLB highpass (38Hz).
/// Outputs momentary (400ms), short-term (3s), integrated (dual-gated).
use super::biquad::Biquad;

const BLOCK_MS: f64 = 100.0;   // measurement block size
const MOM_MS: f64 = 400.0;     // momentary window
const ST_MS: f64 = 3000.0;     // short-term window

pub struct LufsMeter {
    sr: f64,
    stage1: Biquad,    // high-shelf pre-filter
    stage2: Biquad,    // RLB highpass

    // Circular MS buffer (100ms blocks)
    blocks: Vec<f64>,
    block_pos: usize,
    sample_count: usize,
    block_samples: usize,
    current_sum: f64,

    // Integrated gating
    int_blocks: Vec<f64>,
    int_pos: usize,
    int_count: usize,
}

impl LufsMeter {
    pub fn new(sr: f64) -> Self {
        let stage1 = Biquad::high_shelf(1681.97, 4.0, sr);
        let stage2 = Biquad::hpf(38.14, sr);
        let block_samples = (sr * BLOCK_MS / 1000.0) as usize;
        let mom_blocks = (MOM_MS / BLOCK_MS) as usize;
        let st_blocks = (ST_MS / BLOCK_MS) as usize;
        let int_blocks = st_blocks * 40; // ~2 minutes of history

        LufsMeter {
            sr,
            stage1,
            stage2,
            blocks: vec![0.0; mom_blocks.max(st_blocks)],
            block_pos: 0,
            sample_count: 0,
            block_samples: block_samples.max(1),
            current_sum: 0.0,
            int_blocks: vec![0.0; int_blocks],
            int_pos: 0,
            int_count: 0,
        }
    }

    /// Feed one sample; returns Some(LufsReadings) every 100ms block.
    pub fn tick(&mut self, x: f32) -> Option<LufsReadings> {
        // K-weighting
        let k = self.stage2.tick(self.stage1.tick(x));
        self.current_sum += (k as f64) * (k as f64);
        self.sample_count += 1;

        if self.sample_count >= self.block_samples {
            let ms = self.current_sum / self.sample_count as f64;
            self.current_sum = 0.0;
            self.sample_count = 0;

            // Store block
            let n = self.blocks.len();
            self.blocks[self.block_pos % n] = ms;
            self.block_pos += 1;

            // Integrated history
            let int_n = self.int_blocks.len();
            self.int_blocks[self.int_pos % int_n] = ms;
            self.int_pos += 1;
            self.int_count = (self.int_count + 1).min(int_n);

            let mom_n = (MOM_MS / BLOCK_MS) as usize;
            let st_n = (ST_MS / BLOCK_MS) as usize;

            let momentary = ms_to_lufs(mean_of_last(&self.blocks, self.block_pos, mom_n));
            let short_term = ms_to_lufs(mean_of_last(&self.blocks, self.block_pos, st_n));
            let integrated = self.compute_integrated();

            return Some(LufsReadings { momentary, short_term, integrated });
        }
        None
    }

    fn compute_integrated(&self) -> f32 {
        if self.int_count < 4 { return -70.0; }
        let n = self.int_count;
        let blocks = &self.int_blocks[..n.min(self.int_blocks.len())];

        // Absolute gate: -70 LUFS
        let abs_gate_ms = lufs_to_ms(-70.0);
        let pass1: Vec<f64> = blocks.iter().copied().filter(|&v| v >= abs_gate_ms).collect();
        if pass1.is_empty() { return -70.0; }

        // Relative gate: -10 LU below ungated mean
        let ungated_lufs = ms_to_lufs(pass1.iter().sum::<f64>() / pass1.len() as f64);
        let rel_gate_ms = lufs_to_ms(ungated_lufs - 10.0);
        let pass2: Vec<f64> = pass1.into_iter().filter(|&v| v >= rel_gate_ms).collect();
        if pass2.is_empty() { return ungated_lufs; }

        ms_to_lufs(pass2.iter().sum::<f64>() / pass2.len() as f64)
    }

    pub fn reset(&mut self) {
        self.blocks.fill(0.0);
        self.int_blocks.fill(0.0);
        self.block_pos = 0;
        self.int_pos = 0;
        self.int_count = 0;
        self.current_sum = 0.0;
        self.sample_count = 0;
        self.stage1.reset();
        self.stage2.reset();
    }
}

#[derive(serde::Serialize, Clone)]
pub struct LufsReadings {
    pub momentary:   f32,
    pub short_term:  f32,
    pub integrated:  f32,
}

fn mean_of_last(buf: &[f64], pos: usize, n: usize) -> f64 {
    let avail = n.min(buf.len()).min(pos);
    if avail == 0 { return 0.0; }
    let sum: f64 = (0..avail)
        .map(|i| buf[(pos + buf.len() - 1 - i) % buf.len()])
        .sum();
    sum / avail as f64
}

#[inline] fn ms_to_lufs(ms: f64) -> f32 {
    if ms <= 1e-10 { return -70.0; }
    (10.0 * ms.log10() - 0.691) as f32
}
#[inline] fn lufs_to_ms(lufs: f32) -> f64 {
    10f64.powf(((lufs as f64) + 0.691) / 10.0)
}

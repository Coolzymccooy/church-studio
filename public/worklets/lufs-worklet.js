/**
 * LUFS Processor — ITU-R BS.1770-4 compliant loudness metering
 *
 * Implements full K-weighting filter chain:
 *   Stage 1: Pre-filter (high-shelf, models acoustic effect of a spherical head)
 *   Stage 2: RLB weighting (highpass at ~38Hz)
 *
 * Outputs momentary (400ms), short-term (3s), and integrated (gated) LUFS.
 * Signal passes through unmodified — this is a pure meter/tap node.
 */
class LUFSProcessor extends AudioWorkletProcessor {
  constructor() {
    super();

    const fs = sampleRate;

    // ── Stage 1: Pre-filter (high-shelf +4 dB at ~1682 Hz) ────────────────────
    const f0 = 1681.974450955533;
    const G  = 3.999843853973347;
    const Q1 = 0.7071752369554196;
    const K1 = Math.tan(Math.PI * f0 / fs);
    const Vh = Math.pow(10, G / 20);
    const Vb = Math.pow(Vh, 0.4996667741545);
    const d1 = 1 + K1 / Q1 + K1 * K1;
    this.b1 = [
      (Vh + Vb * K1 / Q1 + K1 * K1) / d1,
      2  * (K1 * K1 - Vh) / d1,
      (Vh - Vb * K1 / Q1 + K1 * K1) / d1,
    ];
    this.a1 = [2 * (K1 * K1 - 1) / d1, (1 - K1 / Q1 + K1 * K1) / d1];

    // ── Stage 2: RLB weighting (highpass at ~38 Hz) ───────────────────────────
    const f1 = 38.13547087602444;
    const Q2 = 0.5003270373238773;
    const K2 = Math.tan(Math.PI * f1 / fs);
    const d2 = 1 + K2 / Q2 + K2 * K2;
    this.b2 = [1 / d2, -2 / d2, 1 / d2];
    this.a2 = [2 * (K2 * K2 - 1) / d2, (1 - K2 / Q2 + K2 * K2) / d2];

    // Filter state: [left, right] × [z1, z2]
    this.z1 = [[0, 0], [0, 0]];
    this.z2 = [[0, 0], [0, 0]];

    // ── Circular MS buffer (3.5s max) ─────────────────────────────────────────
    const bufLen = Math.ceil(fs * 3.5);
    this.msBuf    = new Float64Array(bufLen);
    this.msBufLen = bufLen;
    this.msWrite  = 0;

    // ── 100ms block accumulator (for integrated gating) ───────────────────────
    this.blockSize   = Math.round(fs * 0.1);
    this.blockAccum  = 0;
    this.blockCount  = 0;
    this.intBlocks   = []; // loudness of each 100ms block

    // ── Report timer ──────────────────────────────────────────────────────────
    this.reportTimer    = 0;
    this.reportInterval = Math.round(fs * 0.1);
  }

  _kWeight(x, ch) {
    // Stage 1
    const s1 = this.z1[ch];
    const y1 = this.b1[0] * x + s1[0];
    s1[0] = this.b1[1] * x - this.a1[0] * y1 + s1[1];
    s1[1] = this.b1[2] * x - this.a1[1] * y1;
    // Stage 2
    const s2 = this.z2[ch];
    const y2 = this.b2[0] * y1 + s2[0];
    s2[0] = this.b2[1] * y1 - this.a2[0] * y2 + s2[1];
    s2[1] = this.b2[2] * y1 - this.a2[1] * y2;
    return y2;
  }

  process(inputs, outputs, parameters) {
    const input  = inputs[0];
    const output = outputs[0];
    if (!input || !input[0]) return true;

    // Pass-through: meter only, no modification
    for (let ch = 0; ch < output.length; ch++) {
      output[ch].set(input[ch] || input[0]);
    }

    const left  = input[0];
    const right = input[1] || input[0];
    const len   = left.length;

    for (let i = 0; i < len; i++) {
      const wL = this._kWeight(left[i],  0);
      const wR = this._kWeight(right[i], 1);
      // ITU-R BS.1770: mean square of K-weighted channels
      const ms = 0.5 * (wL * wL + wR * wR);

      this.msBuf[this.msWrite % this.msBufLen] = ms;
      this.msWrite++;

      // Accumulate 100ms block for integrated gating
      this.blockAccum += ms;
      this.blockCount++;
      if (this.blockCount >= this.blockSize) {
        const blockL = -0.691 + 10 * Math.log10(this.blockAccum / this.blockCount + 1e-20);
        this.intBlocks.push(blockL);
        if (this.intBlocks.length > 1800) this.intBlocks.shift(); // ~3 minutes max
        this.blockAccum = 0;
        this.blockCount = 0;
      }

      this.reportTimer++;
    }

    if (this.reportTimer >= this.reportInterval) {
      this.reportTimer = 0;
      const pos = this.msWrite;

      // Momentary LUFS (400ms window)
      const momN   = Math.min(pos, Math.round(sampleRate * 0.4));
      let momSum   = 0;
      for (let i = 0; i < momN; i++) {
        momSum += this.msBuf[(pos - momN + i + this.msBufLen * 4) % this.msBufLen];
      }
      const momentary = momN > 0 ? -0.691 + 10 * Math.log10(momSum / momN + 1e-20) : -70;

      // Short-term LUFS (3s window)
      const stN    = Math.min(pos, Math.round(sampleRate * 3));
      let stSum    = 0;
      for (let i = 0; i < stN; i++) {
        stSum += this.msBuf[(pos - stN + i + this.msBufLen * 4) % this.msBufLen];
      }
      const shortTerm = stN > 0 ? -0.691 + 10 * Math.log10(stSum / stN + 1e-20) : -70;

      // Integrated LUFS (BS.1770-4 dual-gate)
      let integrated = -70;
      if (this.intBlocks.length > 0) {
        // Absolute gate: blocks > -70 LUFS
        const absGated = this.intBlocks.filter(l => l > -70);
        if (absGated.length > 0) {
          const absAvg = absGated.reduce((s, l) => s + Math.pow(10, l / 10), 0) / absGated.length;
          // Relative gate: -10 LU below ungated average
          const relThresh = 10 * Math.log10(absAvg) - 10;
          const relGated  = absGated.filter(l => l > relThresh);
          if (relGated.length > 0) {
            const intAvg = relGated.reduce((s, l) => s + Math.pow(10, l / 10), 0) / relGated.length;
            integrated = 10 * Math.log10(intAvg);
          }
        }
      }

      this.port.postMessage({
        momentary:  Math.max(-70, isFinite(momentary)  ? momentary  : -70),
        shortTerm:  Math.max(-70, isFinite(shortTerm)  ? shortTerm  : -70),
        integrated: Math.max(-70, isFinite(integrated) ? integrated : -70),
      });
    }

    return true;
  }
}

registerProcessor('lufs-processor', LUFSProcessor);

/**
 * Spectral Dereverberation Processor
 *
 * True dereverberation via overlap-add FFT + minimum statistics noise tracking.
 *
 * How it works:
 *   - Audio is processed in overlapping frames (1024-pt FFT, 75% overlap)
 *   - Per frequency bin, the reverb tail is estimated using a two-component model:
 *       Fast estimate: tracks rapid reverb buildup (mirrors direct sound energy × factor)
 *       Slow estimate: tracks the slow-decaying tail (uses a running minimum)
 *   - The reverb estimate is subtracted from the magnitude spectrum (spectral subtraction)
 *   - A spectral floor prevents complete suppression (avoids musical noise artifacts)
 *   - Phase is preserved from the original signal — only magnitude is modified
 *
 * This replaces the previous approach of lowpass-filter + gain ducking, which was
 * only a rough proxy for dereverberation.
 *
 * Latency: (N - HOP) = 768 samples ≈ 16ms at 48kHz
 */
class DereverbProcessor extends AudioWorkletProcessor {
  constructor() {
    super();

    this.N    = 1024;
    this.HOP  = 256;          // 75% overlap
    this.HALF = this.N / 2 + 1;

    // Analysis/synthesis Hann window
    this.win = new Float32Array(this.N);
    for (let i = 0; i < this.N; i++) {
      this.win[i] = 0.5 * (1 - Math.cos(2 * Math.PI * i / this.N));
    }
    // OLA scaling constant (WOLA with 75% overlap, sum of squared windows = N/HOP)
    this.scale = 1 / (this.N / this.HOP * 0.375); // 0.375 = squared Hann mean with 75% OL

    // Input circular buffer
    this.inBuf  = new Float32Array(this.N * 2);
    this.inPos  = 0;
    this.inFill = 0;

    // Output OLA buffer (must be large enough: N * 2 frames)
    this.outBuf   = new Float32Array(this.N * 8);
    this.outRead  = 0;
    this.outWrite = 0;

    // FFT work arrays
    this.re = new Float32Array(this.N);
    this.im = new Float32Array(this.N);

    // Reverb estimation per bin
    this.reverbFast = new Float32Array(this.HALF); // fast-tracking component
    this.reverbSlow = new Float32Array(this.HALF); // slow-decay minimum statistics
    this.reverbMin  = new Float32Array(this.HALF); // running minimum per bin
    this.minAge     = new Uint16Array(this.HALF);  // frames since last minimum update

    // Processing parameters (controlled via port messages)
    this.strength = 1.5;   // subtraction strength (1.0 = conservative, 2.0 = aggressive)
    this.floor    = 0.08;  // spectral floor ratio (prevents over-suppression / musical noise)
    this.enabled  = true;

    this.frameCount = 0;
    this.MIN_UPDATE_FRAMES = 8; // update running minimum every N frames

    this.port.onmessage = (e) => {
      if (e.data.strength !== undefined) this.strength = Math.max(0, e.data.strength);
      if (e.data.enabled  !== undefined) this.enabled  = e.data.enabled;
      if (e.data.floor    !== undefined) this.floor    = Math.max(0.01, e.data.floor);
    };

    // Pre-fill output write head to compensate for analysis latency
    this.outWrite = (this.N - this.HOP);
  }

  // ── Radix-2 DIT Cooley-Tukey FFT (in-place) ─────────────────────────────────
  _fft(re, im, inverse) {
    const N = re.length;
    // Bit-reversal
    for (let i = 1, j = 0; i < N; i++) {
      let bit = N >> 1;
      for (; j & bit; bit >>= 1) j ^= bit;
      j ^= bit;
      if (i < j) {
        let t;
        t = re[i]; re[i] = re[j]; re[j] = t;
        t = im[i]; im[i] = im[j]; im[j] = t;
      }
    }
    // Butterfly
    for (let s = 2; s <= N; s <<= 1) {
      const angle = (inverse ? 2 : -2) * Math.PI / s;
      const wR = Math.cos(angle);
      const wI = Math.sin(angle);
      for (let k = 0; k < N; k += s) {
        let uR = 1, uI = 0;
        for (let j = 0; j < s >> 1; j++) {
          const p = k + j, q = p + (s >> 1);
          const tR = uR * re[q] - uI * im[q];
          const tI = uR * im[q] + uI * re[q];
          re[q] = re[p] - tR; im[q] = im[p] - tI;
          re[p] += tR;        im[p] += tI;
          const nr = uR * wR - uI * wI;
          uI = uR * wI + uI * wR;
          uR = nr;
        }
      }
    }
    if (inverse) {
      for (let i = 0; i < N; i++) { re[i] /= N; im[i] /= N; }
    }
  }

  _processFrame() {
    // ── Copy windowed frame from input buffer ─────────────────────────────────
    for (let i = 0; i < this.N; i++) {
      const idx = (this.inPos - this.N + i + this.inBuf.length * 4) % this.inBuf.length;
      this.re[i] = this.inBuf[idx] * this.win[i];
      this.im[i] = 0;
    }

    // ── Forward FFT ───────────────────────────────────────────────────────────
    this._fft(this.re, this.im, false);

    // ── Spectral dereverberation ──────────────────────────────────────────────
    for (let k = 0; k < this.HALF; k++) {
      const mag   = Math.sqrt(this.re[k] * this.re[k] + this.im[k] * this.im[k]);
      const phase = Math.atan2(this.im[k], this.re[k]);

      if (this.enabled && this.strength > 0) {
        // Two-component reverb tail estimate:
        // Fast: tracks reverb buildup quickly (direct + early reflections)
        // Slow: tracks the long reverb tail via minimum statistics
        this.reverbFast[k] = Math.max(this.reverbFast[k] * 0.97, mag * 0.12);
        this.reverbSlow[k] = Math.max(this.reverbSlow[k] * 0.992, mag * 0.04);

        // Running minimum per bin (minimum statistics estimator)
        if (mag < this.reverbMin[k] || this.minAge[k] > this.MIN_UPDATE_FRAMES) {
          this.reverbMin[k] = mag * 0.85;
          this.minAge[k]    = 0;
        } else {
          this.minAge[k]++;
        }

        // Combined reverb estimate: max of fast and slow components, floored by min
        const reverbEst = Math.max(
          Math.max(this.reverbFast[k], this.reverbSlow[k]),
          this.reverbMin[k] * 1.4,
        );

        // Spectral subtraction with soft floor
        const suppressed = Math.max(
          mag - this.strength * reverbEst,
          mag * this.floor,
        );

        // Reconstruct with original phase
        this.re[k] = suppressed * Math.cos(phase);
        this.im[k] = suppressed * Math.sin(phase);
      }

      // Enforce Hermitian symmetry for real IFFT
      if (k > 0 && k < this.N >> 1) {
        this.re[this.N - k] =  this.re[k];
        this.im[this.N - k] = -this.im[k];
      }
    }

    // ── Inverse FFT ──────────────────────────────────────────────────────────
    this._fft(this.re, this.im, true);

    // ── Overlap-add into output buffer ───────────────────────────────────────
    for (let i = 0; i < this.N; i++) {
      const idx = (this.outWrite + i) % this.outBuf.length;
      this.outBuf[idx] += this.re[i] * this.win[i] * this.scale;
    }
    this.outWrite = (this.outWrite + this.HOP) % this.outBuf.length;

    this.frameCount++;
  }

  process(inputs, outputs) {
    const input  = inputs[0];
    const output = outputs[0];
    if (!input || !input[0] || !output || !output[0]) return true;

    const inp = input[0];
    const out = output[0];
    const len = inp.length; // 128 samples per block

    // ── Feed input into ring buffer ───────────────────────────────────────────
    for (let i = 0; i < len; i++) {
      this.inBuf[this.inPos % this.inBuf.length] = inp[i];
      this.inPos++;
      this.inFill++;
    }

    // ── Process all complete HOP-sized frames ─────────────────────────────────
    while (this.inFill >= this.HOP) {
      this._processFrame();
      this.inFill -= this.HOP;
    }

    // ── Read processed samples from output buffer ─────────────────────────────
    for (let i = 0; i < len; i++) {
      const idx = (this.outRead + i) % this.outBuf.length;
      out[i]           = this.outBuf[idx];
      this.outBuf[idx] = 0; // clear slot after reading
    }
    this.outRead = (this.outRead + len) % this.outBuf.length;

    return true;
  }
}

registerProcessor('dereverb-processor', DereverbProcessor);

/**
 * Dynamic De-esser Processor
 *
 * A true sidechain de-esser: far more transparent than a fixed notch filter.
 *
 * How it works:
 *   1. A bandpass filter (centred on the sibilance frequency) taps the sidechain
 *   2. An envelope follower tracks the energy in that band
 *   3. When sibilance energy exceeds the threshold, a gain reduction is applied
 *      to the FULL signal — but ONLY during that moment
 *   4. When the sibilance event ends, gain returns to unity quickly (release)
 *
 * Result: "ss", "sh", "ch" sounds are tamed without dulling normal consonants
 * or removing air/presence from the voice permanently.
 */
class DynamicDesserProcessor extends AudioWorkletProcessor {
  static get parameterDescriptors() {
    return [
      { name: 'threshold',  defaultValue: -24,  minValue: -60,   maxValue: 0 },
      { name: 'ratio',      defaultValue: 8,    minValue: 1,     maxValue: 40 },
      { name: 'frequency',  defaultValue: 7500, minValue: 3000,  maxValue: 16000 },
      { name: 'bandwidth',  defaultValue: 3000, minValue: 500,   maxValue: 10000 },
      { name: 'attackMs',   defaultValue: 1,    minValue: 0.1,   maxValue: 20 },
      { name: 'releaseMs',  defaultValue: 40,   minValue: 5,     maxValue: 300 },
      { name: 'enabled',    defaultValue: 1,    minValue: 0,     maxValue: 1 },
    ];
  }

  constructor() {
    super();

    // Bandpass filter state (Direct Form II transposed), per channel
    this.bpState = [new Float32Array(2), new Float32Array(2)];
    this.scEnv   = new Float32Array(2); // sidechain envelope per channel
    this.grGain  = 1.0;                 // gain reduction (1 = no reduction)

    // Cached filter coefficients — recomputed only on param change
    this._lastFreq = -1;
    this._lastBw   = -1;
    this.bpCoeffs  = { b0: 0, b2: 0, a1: 0, a2: 0 };
  }

  _computeBandpass(freq, bw) {
    if (freq === this._lastFreq && bw === this._lastBw) return;
    this._lastFreq = freq;
    this._lastBw   = bw;
    const fs   = sampleRate;
    const Q    = Math.max(freq / Math.max(bw, 1), 0.1);
    const w0   = 2 * Math.PI * freq / fs;
    const sinW = Math.sin(w0);
    const cosW = Math.cos(w0);
    const alph = sinW / (2 * Q);
    const a0   = 1 + alph;
    this.bpCoeffs = {
      b0:  alph / a0,
      b2: -alph / a0,
      a1: -2 * cosW / a0,
      a2: (1 - alph) / a0,
    };
  }

  _bandpass(x, ch) {
    const { b0, b2, a1, a2 } = this.bpCoeffs;
    const s = this.bpState[ch];
    const y = b0 * x + s[0];
    s[0] = /* b1 = 0 */ -a1 * y + s[1];
    s[1] = b2 * x - a2 * y;
    return y;
  }

  process(inputs, outputs, parameters) {
    const input  = inputs[0];
    const output = outputs[0];
    if (!input || !input[0]) return true;

    const enabled = (parameters.enabled[0] ?? 1) >= 0.5;
    if (!enabled) {
      for (let ch = 0; ch < output.length; ch++) output[ch].set(input[ch] || input[0]);
      return true;
    }

    const threshold  = parameters.threshold[0] ?? -24;
    const ratio      = parameters.ratio[0]     ?? 8;
    const freq       = parameters.frequency[0] ?? 7500;
    const bw         = parameters.bandwidth[0] ?? 3000;
    const attackMs   = parameters.attackMs[0]  ?? 1;
    const releaseMs  = parameters.releaseMs[0] ?? 40;

    this._computeBandpass(freq, bw);

    const threshLin    = Math.pow(10, threshold / 20);
    const attackCoeff  = Math.exp(-1 / (sampleRate * attackMs   / 1000));
    const releaseCoeff = Math.exp(-1 / (sampleRate * releaseMs  / 1000));
    const nCh          = Math.min(input.length, output.length, 2);
    const len          = input[0].length;

    for (let i = 0; i < len; i++) {
      // ── Sidechain detection: peak envelope per channel ─────────────────────
      let maxEnv = 0;
      for (let ch = 0; ch < nCh; ch++) {
        const sc  = this._bandpass(input[ch][i], ch);
        const abs = Math.abs(sc);
        // Peak hold with asymmetric attack/release
        if (abs > this.scEnv[ch]) {
          this.scEnv[ch] = abs + (this.scEnv[ch] - abs) * attackCoeff;
        } else {
          this.scEnv[ch] = abs + (this.scEnv[ch] - abs) * releaseCoeff;
        }
        maxEnv = Math.max(maxEnv, this.scEnv[ch]);
      }

      // ── Gain reduction: compress only when sibilance detected ──────────────
      let targetGR = 1.0;
      if (maxEnv > threshLin) {
        const excessDb = 20 * Math.log10(maxEnv / threshLin);
        const grDb     = -excessDb * (1 - 1 / ratio);
        targetGR = Math.pow(10, grDb / 20);
      }

      // Smooth the GR envelope (attack = fast grab, release = slow return)
      if (targetGR < this.grGain) {
        this.grGain += (targetGR - this.grGain) * (1 - attackCoeff);
      } else {
        this.grGain += (targetGR - this.grGain) * (1 - releaseCoeff);
      }

      // Apply gain reduction to all channels
      for (let ch = 0; ch < nCh; ch++) {
        output[ch][i] = (input[ch] || input[0])[i] * this.grGain;
      }
    }

    // Report GR level for metering UI (negative = reduction)
    this.port.postMessage({ grDb: 20 * Math.log10(Math.max(this.grGain, 1e-9)) });

    return true;
  }
}

registerProcessor('dynamic-desser-processor', DynamicDesserProcessor);

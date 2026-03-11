/**
 * Lookahead Gate Processor
 *
 * Delays the output signal by N ms while looking ahead at the INCOMING signal
 * to detect voice. This lets the gate open BEFORE the voice actually arrives
 * at the output — eliminating the clipped first-syllable problem.
 *
 * How it works:
 *   - Incoming signal → delay ring buffer (adds N ms of latency)
 *   - Simultaneously, the current (non-delayed) input is inspected for energy
 *   - If energy > threshold → gate opens NOW
 *   - Delayed signal exits N ms later → gate is already fully open → no clipped attack
 */
class LookaheadGateProcessor extends AudioWorkletProcessor {
  static get parameterDescriptors() {
    return [
      { name: 'threshold',   defaultValue: -45,  minValue: -80,   maxValue: 0 },
      { name: 'lookaheadMs', defaultValue: 20,   minValue: 0,     maxValue: 50 },
      { name: 'attackMs',    defaultValue: 2,    minValue: 0.5,   maxValue: 50 },
      { name: 'releaseMs',   defaultValue: 250,  minValue: 10,    maxValue: 3000 },
      { name: 'holdMs',      defaultValue: 200,  minValue: 0,     maxValue: 1000 },
      { name: 'enabled',     defaultValue: 1,    minValue: 0,     maxValue: 1 },
    ];
  }

  constructor() {
    super();

    // Ring buffer large enough for max 50ms lookahead at any sample rate
    const maxSamp = Math.ceil(sampleRate * 0.055) + 512;
    this.delayL  = new Float32Array(maxSamp);
    this.delayR  = new Float32Array(maxSamp);
    this.bufLen  = maxSamp;
    this.writePos = 0;

    // Gate state
    this.gateGain    = 0.0;
    this.holdRemain  = 0;

    // RMS smoother for detection
    this.rmsSmooth   = 0.0;
    this.rmsTau      = Math.exp(-1 / (sampleRate * 0.005)); // 5ms RMS window
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

    const threshold    = parameters.threshold[0]   ?? -45;
    const lookaheadMs  = parameters.lookaheadMs[0] ?? 20;
    const attackMs     = parameters.attackMs[0]    ?? 2;
    const releaseMs    = parameters.releaseMs[0]   ?? 250;
    const holdMs       = parameters.holdMs[0]      ?? 200;

    const threshLin      = Math.pow(10, threshold / 20);
    const lookaheadSamp  = Math.min(Math.round(sampleRate * lookaheadMs / 1000), this.bufLen - 1);
    const attackCoeff    = Math.exp(-1 / (sampleRate * attackMs   / 1000));
    const releaseCoeff   = Math.exp(-1 / (sampleRate * releaseMs  / 1000));
    const holdSamples    = Math.round(sampleRate * holdMs / 1000);

    const left  = input[0];
    const right = input[1] || input[0];
    const len   = left.length;

    for (let i = 0; i < len; i++) {
      const wp = this.writePos % this.bufLen;

      // Write into delay buffers
      this.delayL[wp] = left[i];
      this.delayR[wp] = right[i];

      // Detect energy from current (lookahead) sample
      const absVal = Math.max(Math.abs(left[i]), Math.abs(right[i]));
      // Smooth RMS for stable detection
      this.rmsSmooth = this.rmsSmooth * this.rmsTau + absVal * (1 - this.rmsTau);

      // Gate decision
      if (this.rmsSmooth > threshLin) {
        this.holdRemain = holdSamples;
        // Attack: fast open
        this.gateGain += (1.0 - this.gateGain) * (1 - attackCoeff);
      } else if (this.holdRemain > 0) {
        this.holdRemain--;
        // Hold: sustain gain
      } else {
        // Release: slow close
        this.gateGain += (0.0 - this.gateGain) * (1 - releaseCoeff);
      }

      // Read delayed signal (signal that arrives 'lookahead' ms after detection decision)
      const rp = (this.writePos - lookaheadSamp + this.bufLen * 8) % this.bufLen;
      if (output[0]) output[0][i] = this.delayL[rp] * this.gateGain;
      if (output[1]) output[1][i] = this.delayR[rp] * this.gateGain;

      this.writePos++;
    }

    // Report gate gain for visualizer
    this.port.postMessage({ gateGain: this.gateGain });

    return true;
  }
}

registerProcessor('lookahead-gate-processor', LookaheadGateProcessor);

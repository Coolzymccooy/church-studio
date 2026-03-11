/**
 * HyperGate AudioWorklet Processor
 * Runs spectral flatness, zero-crossing rate, formant detection, and gating
 * directly on the audio render thread for sub-1ms latency.
 *
 * Communicates with main thread via MessagePort for threshold/mode changes.
 */

// --- Minimal Radix-2 FFT (in-place, complex) ---
function fftRadix2(re, im) {
  const n = re.length;
  // Bit-reversal permutation
  for (let i = 1, j = 0; i < n; i++) {
    let bit = n >> 1;
    while (j & bit) {
      j ^= bit;
      bit >>= 1;
    }
    j ^= bit;
    if (i < j) {
      [re[i], re[j]] = [re[j], re[i]];
      [im[i], im[j]] = [im[j], im[i]];
    }
  }
  // Cooley-Tukey butterflies
  for (let len = 2; len <= n; len <<= 1) {
    const halfLen = len >> 1;
    const angle = (-2 * Math.PI) / len;
    const wRe = Math.cos(angle);
    const wIm = Math.sin(angle);
    for (let i = 0; i < n; i += len) {
      let curRe = 1, curIm = 0;
      for (let j = 0; j < halfLen; j++) {
        const tRe = curRe * re[i + j + halfLen] - curIm * im[i + j + halfLen];
        const tIm = curRe * im[i + j + halfLen] + curIm * re[i + j + halfLen];
        re[i + j + halfLen] = re[i + j] - tRe;
        im[i + j + halfLen] = im[i + j] - tIm;
        re[i + j] += tRe;
        im[i + j] += tIm;
        const newCurRe = curRe * wRe - curIm * wIm;
        curIm = curRe * wIm + curIm * wRe;
        curRe = newCurRe;
      }
    }
  }
}

class HyperGateProcessor extends AudioWorkletProcessor {
  constructor() {
    super();

    // FFT config
    this.fftSize = 512;
    this.sampleBuffer = new Float32Array(this.fftSize);
    this.bufferIndex = 0;

    // Gate state
    this.gateGain = 0.015;
    this.voiceHoldUntil = 0;
    this.gateHoldUntil = 0;
    this.frameCount = 0;

    // Settings (updated via messages from main thread)
    this.threshold = -50;
    this.gateMode = 'speech'; // 'balanced' | 'speech'
    this.enabled = true;
    this.bypassed = false;

    // Spectral results (updated every FFT cycle)
    this.spectralFlatness = 1;
    this.zeroCrossingRate = 0;
    this.formantScore = 0;
    this.voiceEnergy = 0;
    this.noiseEnergy = 0;
    this.voiceRatio = 0;
    this.lowAvg = 0;
    this.highBias = 0;
    this.highOnlyNoise = false;
    this.db = -100;

    // Hann window (pre-computed)
    this.hannWindow = new Float32Array(this.fftSize);
    for (let i = 0; i < this.fftSize; i++) {
      this.hannWindow[i] = 0.5 * (1 - Math.cos((2 * Math.PI * i) / (this.fftSize - 1)));
    }

    // Mic fingerprint (sent from main thread after learning session)
    this.micFingerprint = null; // { bandMeans: Float32Array(8), bandStdDevs: Float32Array(8) }

    // Listen for config updates from main thread
    this.port.onmessage = (event) => {
      const data = event.data;
      if (data.type === 'config') {
        if (data.threshold !== undefined) this.threshold = data.threshold;
        if (data.gateMode !== undefined) this.gateMode = data.gateMode;
        if (data.enabled !== undefined) this.enabled = data.enabled;
        if (data.bypassed !== undefined) this.bypassed = data.bypassed;
      }
      // Receive learned mic fingerprint from main thread
      if (data.type === 'fingerprint') {
        this.micFingerprint = {
          bandMeans: new Float32Array(data.bandMeans),
          bandStdDevs: new Float32Array(data.bandStdDevs),
        };
      }
    };
  }

  runSpectralAnalysis() {
    const n = this.fftSize;
    const binHz = (sampleRate / 2) / (n / 2);

    // Apply Hann window and prepare FFT input
    const re = new Float32Array(n);
    const im = new Float32Array(n);
    for (let i = 0; i < n; i++) {
      re[i] = this.sampleBuffer[i] * this.hannWindow[i];
      im[i] = 0;
    }

    fftRadix2(re, im);

    // Compute magnitude spectrum (first half only)
    const halfN = n / 2;
    const mag = new Float32Array(halfN);
    for (let i = 0; i < halfN; i++) {
      mag[i] = Math.sqrt(re[i] * re[i] + im[i] * im[i]);
    }

    // --- Band averages ---
    const bandAvg = (startHz, endHz) => {
      const s = Math.max(0, Math.floor(startHz / binHz));
      const e = Math.min(halfN - 1, Math.ceil(endHz / binHz));
      if (e <= s) return 0;
      let sum = 0, count = 0;
      for (let i = s; i <= e; i++) { sum += mag[i]; count++; }
      return count ? sum / count : 0;
    };

    this.voiceEnergy = bandAvg(300, 3400);
    const lowNoise = bandAvg(40, 180);
    const highNoise = bandAvg(6000, 12000);
    this.noiseEnergy = (lowNoise + highNoise) / 2;
    this.voiceRatio = (this.voiceEnergy + 1) / (this.noiseEnergy + 1);

    // Low/high split for basic energy analysis
    const splitIndex = Math.floor(halfN * 0.4);
    let lowE = 0, highE = 0;
    for (let i = 0; i < halfN; i++) {
      if (i < splitIndex) lowE += mag[i]; else highE += mag[i];
    }
    this.lowAvg = lowE / (splitIndex || 1);
    const highAvg = highE / (halfN - splitIndex || 1);
    this.highBias = highAvg / (this.lowAvg + 1e-6);
    this.highOnlyNoise = this.highBias > 1.8 && this.lowAvg < 35;

    // --- Spectral Flatness (200-5000 Hz) ---
    {
      const sfStart = Math.max(0, Math.floor(200 / binHz));
      const sfEnd = Math.min(halfN - 1, Math.ceil(5000 / binHz));
      let logSum = 0, linSum = 0, count = 0;
      for (let i = sfStart; i <= sfEnd; i++) {
        const v = Math.max(mag[i], 0.0001);
        logSum += Math.log(v);
        linSum += v;
        count++;
      }
      if (count > 0 && linSum > 0) {
        this.spectralFlatness = Math.exp(logSum / count) / (linSum / count);
      } else {
        this.spectralFlatness = 1;
      }
    }

    // --- Zero-Crossing Rate ---
    {
      let crossings = 0;
      for (let i = 1; i < n; i++) {
        if ((this.sampleBuffer[i - 1] >= 0 && this.sampleBuffer[i] < 0) ||
            (this.sampleBuffer[i - 1] < 0 && this.sampleBuffer[i] >= 0)) {
          crossings++;
        }
      }
      this.zeroCrossingRate = crossings / n;
    }

    // --- Formant Structure Score ---
    {
      const f1 = bandAvg(300, 900);
      const f2 = bandAvg(900, 2500);
      const f3 = bandAvg(2500, 3500);
      const valley1 = bandAvg(180, 300);
      const valley2 = bandAvg(3500, 5000);
      const avgValley = (valley1 + valley2) / 2 + 0.0001;

      let peaks = 0;
      if (f1 / avgValley > 1.3) peaks++;
      if (f2 / avgValley > 1.3) peaks++;
      if (f3 / avgValley > 1.3) peaks++;

      const maxF = Math.max(f1, f2, f3);
      const minF = Math.min(f1, f2, f3);
      const spread = maxF > 0 ? minF / maxF : 0;

      this.formantScore = (peaks >= 2 && spread > 0.2) ? peaks + spread : 0;
    }
  }

  process(inputs, outputs) {
    const input = inputs[0];
    const output = outputs[0];

    if (!input || input.length === 0 || !input[0]) {
      return true;
    }

    const inputChannel = input[0];
    const outputChannel = output[0];
    const blockSize = inputChannel.length; // typically 128

    // If disabled or bypassed, pass through
    if (!this.enabled || this.bypassed) {
      for (let i = 0; i < blockSize; i++) {
        outputChannel[i] = inputChannel[i];
      }
      this.gateGain = 1.0;
      return true;
    }

    // Accumulate samples into FFT buffer
    for (let i = 0; i < blockSize; i++) {
      this.sampleBuffer[this.bufferIndex] = inputChannel[i];
      this.bufferIndex = (this.bufferIndex + 1) % this.fftSize;
    }

    // Compute RMS dB for this block
    let sumSq = 0;
    for (let i = 0; i < blockSize; i++) {
      sumSq += inputChannel[i] * inputChannel[i];
    }
    const rms = Math.sqrt(sumSq / blockSize);
    this.db = 20 * Math.log10(rms || 0.00001);

    // Run FFT analysis every fftSize samples (every ~10ms at 48kHz)
    this.frameCount += blockSize;
    if (this.frameCount >= this.fftSize) {
      this.frameCount = 0;
      this.runSpectralAnalysis();

      // Send metrics to main thread for UI (throttled by FFT cycle)
      this.port.postMessage({
        type: 'metrics',
        db: this.db,
        spectralFlatness: this.spectralFlatness,
        zeroCrossingRate: this.zeroCrossingRate,
        formantScore: this.formantScore,
        voiceRatio: this.voiceRatio,
        voiceEnergy: this.voiceEnergy,
        gateClosed: this.gateGain < 0.12,
        gateGain: this.gateGain,
      });
    }

    // --- Gate Decision ---
    const threshold = this.threshold;
    const openThreshold = threshold + 3;
    const closeThreshold = threshold - 4;
    const minGateGain = 0.015;
    const now = currentTime * 1000; // seconds to ms

    // --- Tiered voice detection (same logic as main thread) ---
    // Tier 1: LOUD + PROXIMITY - very loud AND voice-band dominant
    const loudOverride = this.db > threshold + 15 && !this.highOnlyNoise && this.voiceRatio > 1.0;

    // Tier 2: BASIC checks (works for all mics)
    const basicVoice =
      !this.highOnlyNoise &&
      this.lowAvg > 10 &&
      this.highBias < 3.0 &&
      this.db > threshold &&
      this.voiceEnergy > 10;

    // Tier 3: Spectral checks (supplementary confidence)
    const isNotNoise = this.spectralFlatness < 0.75;
    const isNotHissing = this.zeroCrossingRate < 0.35;
    const hasFormants = this.formantScore > 0;
    const hasVoiceEnergy = this.voiceEnergy > 15 && this.voiceRatio > 1.15;

    // Mic fingerprint match score (worklet-side)
    let micMatchScore = 0.5; // neutral if no fingerprint yet
    if (this.micFingerprint && this.sampleBuffer) {
      const { bandMeans, bandStdDevs } = this.micFingerprint;
      const n = this.fftSize;
      const binHz = (sampleRate / 2) / (n / 2);
      // Compute same 8-band energy as main thread
      const bandRanges = [[40,180],[180,400],[400,900],[900,2000],[2000,3500],[3500,5000],[5000,8000],[8000,12000]];
      let totalDev = 0;
      let matched = 0;
      for (let b = 0; b < bandRanges.length; b++) {
        const [lo, hi] = bandRanges[b];
        const startBin = Math.floor(lo / binHz);
        const endBin = Math.min(Math.ceil(hi / binHz), n / 2);
        let sum = 0; let count = 0;
        for (let i = startBin; i < endBin; i++) {
          sum += Math.abs(this.sampleBuffer[i] || 0);
          count++;
        }
        const bandVal = count > 0 ? sum / count : 0;
        const mean = bandMeans[b] || 0;
        const std = bandStdDevs[b] || 1;
        if (std > 0.0001) {
          const dev = Math.abs(bandVal - mean) / std;
          totalDev += dev;
          if (dev < 2) matched++;
        }
      }
      const matchRatio = matched / bandRanges.length;
      micMatchScore = matchRatio * (1 - Math.min(1, totalDev / (bandRanges.length * 3)));
    }

    // Scoring system (matches main-thread logic)
    let voiceScore = 0;
    if (this.db > threshold) voiceScore += 1;
    if (this.db > openThreshold) voiceScore += 1;
    if (isNotNoise) voiceScore += 1;
    if (isNotHissing) voiceScore += 1;
    if (hasFormants) voiceScore += 2;
    if (hasVoiceEnergy) voiceScore += 2;
    if (this.voiceRatio > 1.0) voiceScore += 1;
    if (!this.highOnlyNoise) voiceScore += 1;
    // Mic fingerprint bonus/penalty
    if (micMatchScore > 0.7) voiceScore += 2;
    else if (micMatchScore > 0.4) voiceScore += 1;
    if (micMatchScore < 0.3) voiceScore -= 2;

    const speechOnlyMode = this.gateMode === 'speech';
    const scoreThreshold = speechOnlyMode ? 6 : 4;

    let voiceCandidate = loudOverride || (basicVoice && voiceScore >= scoreThreshold);

    if (speechOnlyMode && !loudOverride) {
      voiceCandidate = voiceCandidate && (hasVoiceEnergy || voiceScore >= 7);
    }

    if (voiceCandidate) {
      this.voiceHoldUntil = now + 350;
    }
    const localVoiceActive = this.voiceHoldUntil > now;

    let target = minGateGain;

    const canOpenForLevel =
      loudOverride ||
      (this.db > openThreshold && voiceScore >= (speechOnlyMode ? 5 : 3));

    if (canOpenForLevel || localVoiceActive) {
      target = 1.0;
      this.gateHoldUntil = now + 250;
    } else if (now < this.gateHoldUntil) {
      target = 1.0;
    } else if (this.db < closeThreshold) {
      target = minGateGain;
    } else {
      const progress = (this.db - closeThreshold) / (openThreshold - closeThreshold || 1);
      target = minGateGain + Math.max(0, Math.min(1, progress)) * (1 - minGateGain);
    }

    if (speechOnlyMode && !localVoiceActive && !loudOverride) {
      target = Math.min(target, 0.05);
    }

    if (localVoiceActive && this.db < threshold) {
      const lift = Math.min(0.5, 0.25 + (threshold - this.db) * 0.025);
      target = Math.max(target, lift);
    }

    // Smooth gate gain (per-sample for zero-latency response)
    const attackCoeff = 0.005;  // ~0.1ms attack at 48kHz
    const releaseCoeff = 0.0008; // ~2ms release at 48kHz

    for (let i = 0; i < blockSize; i++) {
      const coeff = target > this.gateGain ? attackCoeff : releaseCoeff;
      this.gateGain += (target - this.gateGain) * coeff;
      outputChannel[i] = inputChannel[i] * this.gateGain;
    }

    return true;
  }
}

registerProcessor('hypergate-processor', HyperGateProcessor);

/**
 * Live view analyser: log-frequency LED bands with peak hold, a dB grid, the voice band, the
 * gate threshold, and a scrolling gate timeline. Pure maths here plus one draw function; the
 * caller owns the canvas and the audio analysis.
 */

export const SCOPE = {
  bands: 40,
  fMin: 40,
  fMax: 16000,
  dbMin: -100, // AnalyserNode default minDecibels: byte 0
  dbMax: -30, // AnalyserNode default maxDecibels: byte 255
  riseRate: 30, // level units per second (fast attack)
  fallRate: 1.6,
  peakHoldS: 0.9,
  peakFallRate: 0.6,
  historyS: 8,
  voiceLo: 300,
  voiceHi: 3400,
};

/** x position (0..1) of frequency f on a log axis. */
export function freqFraction(f, fMin = SCOPE.fMin, fMax = SCOPE.fMax) {
  const c = Math.min(fMax, Math.max(fMin, f));
  return Math.log(c / fMin) / Math.log(fMax / fMin);
}

/** y fraction (0 bottom .. 1 top) of a dB value on the analyser scale. */
export function dbFraction(db, dbMin = SCOPE.dbMin, dbMax = SCOPE.dbMax) {
  return Math.min(1, Math.max(0, (db - dbMin) / (dbMax - dbMin)));
}

/** Log-spaced bands, each mapped to the FFT bins it covers (at least one bin). */
export function bandEdges(sampleRate, binCount, n = SCOPE.bands, fMin = SCOPE.fMin, fMax = SCOPE.fMax) {
  const binHz = sampleRate / 2 / binCount;
  const ratio = Math.pow(fMax / fMin, 1 / n);
  const bands = [];
  for (let i = 0; i < n; i += 1) {
    const fLo = fMin * Math.pow(ratio, i);
    const fHi = fLo * ratio;
    const lo = Math.min(binCount - 1, Math.max(0, Math.floor(fLo / binHz)));
    const hi = Math.min(binCount - 1, Math.max(lo, Math.ceil(fHi / binHz) - 1));
    bands.push({ fLo, fHi, centre: Math.sqrt(fLo * fHi), lo, hi });
  }
  return bands;
}

/** Each band's level 0..1: the loudest bin in it. */
export function bandLevels(bytes, bands) {
  return bands.map(({ lo, hi }) => {
    let max = 0;
    for (let i = lo; i <= hi; i += 1) if (bytes[i] > max) max = bytes[i];
    return max / 255;
  });
}

export function createScope(n = SCOPE.bands) {
  return {
    levels: new Array(n).fill(0),
    peaks: new Array(n).fill(0),
    holds: new Array(n).fill(0),
    history: [],
    historyClock: 0,
  };
}

/**
 * Advances the ballistics by `dt` seconds toward `targets`, and appends one timeline sample
 * ({ level, state }) every `sampleEvery` seconds. Returns a new scope; the input is not changed.
 */
export function stepScope(scope, targets, dt, sample = null, sampleEvery = 1 / 30) {
  const levels = scope.levels.map((cur, i) => {
    const t = targets[i] ?? 0;
    return t > cur ? Math.min(t, cur + SCOPE.riseRate * dt) : Math.max(t, cur - SCOPE.fallRate * dt);
  });
  const peaks = [];
  const holds = [];
  scope.peaks.forEach((p, i) => {
    if (levels[i] >= p) {
      peaks.push(levels[i]);
      holds.push(SCOPE.peakHoldS);
    } else if (scope.holds[i] > 0) {
      peaks.push(p);
      holds.push(scope.holds[i] - dt);
    } else {
      peaks.push(Math.max(levels[i], p - SCOPE.peakFallRate * dt));
      holds.push(0);
    }
  });
  let { history, historyClock } = scope;
  historyClock += dt;
  if (sample && historyClock >= sampleEvery) {
    const max = Math.round(SCOPE.historyS / sampleEvery);
    history = [...history.slice(-(max - 1)), sample];
    historyClock = 0;
  }
  return { levels, peaks, holds, history, historyClock };
}

/** Timeline state word for the current frame. */
export function gateState({ denoise, bypassed, gateClosed, voiceActive }) {
  if (bypassed) return 'bypass';
  if (!denoise) return voiceActive ? 'voice' : 'off';
  if (gateClosed) return 'held';
  return voiceActive ? 'voice' : 'open';
}

const SEG = 4;
const GAP = 2;

function bandColour(p, state, inVoice) {
  if (state === 'bypass') return p.dim;
  if (state === 'voice') return inVoice ? p.g : p.gDim;
  if (state === 'held') return inVoice ? p.a : p.aDim;
  return inVoice ? p.text : p.dim;
}

function stateColour(p, state) {
  if (state === 'voice') return p.g;
  if (state === 'held') return p.a;
  if (state === 'open') return p.text;
  return p.dim;
}

const STATE_WORD = { voice: 'gate open · voice', open: 'gate open', held: 'gate closed', off: 'gate off', bypass: 'bypass · raw' };
const FREQ_LABELS = [[50, '50'], [100, '100'], [200, '200'], [500, '500'], [1000, '1k'], [2000, '2k'], [5000, '5k'], [10000, '10k']];
const DB_LINES = [-40, -50, -60, -70, -80, -90];

function label(ctx, p, text, x, y, align = 'left', colour = p.dim, size = 11) {
  ctx.font = `400 ${size}px ${p.mono}`;
  ctx.textAlign = align;
  ctx.fillStyle = colour;
  ctx.fillText(text, x, y);
}

function drawHeader(ctx, p, f, x0, w) {
  const colour = stateColour(p, f.state);
  ctx.fillStyle = colour;
  ctx.fillRect(x0, 14, 10, 10);
  label(ctx, p, STATE_WORD[f.state], x0 + 18, 24, 'left', f.state === 'off' || f.state === 'bypass' ? p.dim : p.text, 14);
  // voice confidence: 10 LED segments
  const segW = 10;
  const mx = x0 + w - 10 * (segW + 2);
  const lit = Math.round(f.confidence / 10);
  for (let i = 0; i < 10; i += 1) {
    ctx.fillStyle = i < lit ? (f.confidence > 50 ? p.g : p.a) : p.grid;
    ctx.fillRect(mx + i * (segW + 2), 14, segW, 10);
  }
  label(ctx, p, `voice ${Math.round(f.confidence)}%`, mx - 10, 24, 'right', p.dim, 12);
  if (Math.abs(f.riderDb) > 0.5) {
    label(ctx, p, `rider ${f.riderDb > 0 ? '+' : ''}${f.riderDb.toFixed(1)} dB`, mx - 130, 24, 'right', p.dim, 12);
  }
}

function drawGrid(ctx, p, box) {
  const { x, y, w, h } = box;
  ctx.fillStyle = p.grid;
  DB_LINES.forEach((db) => {
    const yy = Math.round(y + h - dbFraction(db) * h) + 0.5;
    ctx.fillRect(x, yy, w, 1);
    label(ctx, p, `${db}`, x - 6, yy + 4, 'right', p.dim, 10);
  });
  const vx = x + freqFraction(SCOPE.voiceLo) * w;
  const vw = x + freqFraction(SCOPE.voiceHi) * w - vx;
  ctx.strokeStyle = p.grid;
  ctx.lineWidth = 1;
  ctx.strokeRect(Math.round(vx) + 0.5, y + 0.5, Math.round(vw), h - 1);
  label(ctx, p, 'voice band', vx + vw / 2, y - 6, 'center', p.dim, 10);
  FREQ_LABELS.forEach(([hz, text]) => {
    label(ctx, p, text, x + freqFraction(hz) * w, y + h + 16, 'center', p.dim, 10);
  });
}

function drawBands(ctx, p, box, f, bands) {
  const { x, y, w, h } = box;
  const n = bands.length;
  const colW = w / n;
  const barW = Math.max(2, colW - 3);
  const segs = Math.floor(h / (SEG + GAP));
  bands.forEach((band, i) => {
    const bx = x + i * colW + (colW - barW) / 2;
    const inVoice = band.centre >= SCOPE.voiceLo && band.centre <= SCOPE.voiceHi;
    const litSegs = Math.round(f.levels[i] * segs);
    // Unlit cells are faint so the lit column reads first, like an LED bar graph.
    ctx.globalAlpha = 0.35;
    ctx.fillStyle = p.grid;
    for (let s = litSegs; s < segs; s += 1) ctx.fillRect(bx, y + h - (s + 1) * (SEG + GAP) + GAP, barW, SEG);
    ctx.globalAlpha = 1;
    ctx.fillStyle = bandColour(p, f.state, inVoice);
    for (let s = 0; s < litSegs; s += 1) ctx.fillRect(bx, y + h - (s + 1) * (SEG + GAP) + GAP, barW, SEG);
    const peakSeg = Math.round(f.peaks[i] * segs);
    if (peakSeg > litSegs && peakSeg > 0) {
      ctx.fillStyle = p.text;
      ctx.fillRect(bx, y + h - peakSeg * (SEG + GAP) + GAP, barW, 2);
    }
  });
}

function drawThreshold(ctx, p, box, thresholdDb) {
  const { x, y, w, h } = box;
  const yy = Math.round(y + h - dbFraction(thresholdDb) * h) + 0.5;
  ctx.strokeStyle = p.a;
  ctx.setLineDash([6, 6]);
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(x, yy);
  ctx.lineTo(x + w, yy);
  ctx.stroke();
  ctx.setLineDash([]);
  label(ctx, p, `gate ${thresholdDb} dB`, x + w, yy - 6, 'right', p.a, 11);
}

function drawTimeline(ctx, p, box, history) {
  const { x, y, w, h } = box;
  ctx.fillStyle = p.grid;
  ctx.fillRect(x, y + h - 1, w, 1);
  const max = Math.round(SCOPE.historyS * 30);
  const colW = w / max;
  const start = max - history.length;
  history.forEach((s, i) => {
    const hh = Math.max(1, s.level * (h - 4));
    ctx.fillStyle = stateColour(p, s.state);
    ctx.fillRect(x + (start + i) * colW, y + h - 1 - hh, Math.max(1, colW - 0.5), hh);
  });
  label(ctx, p, `${SCOPE.historyS} s`, x, y + h + 14, 'left', p.dim, 10);
  label(ctx, p, 'now', x + w, y + h + 14, 'right', p.dim, 10);
}

/**
 * Draws one frame. `size` is { w, h } in CSS px (the context is already scaled for the device
 * pixel ratio). `f` = { levels, peaks, history, state, confidence, riderDb, thresholdDb|null }.
 */
export function drawScope(ctx, p, size, f, bands) {
  const { w, h } = size;
  ctx.fillStyle = p.bg;
  ctx.fillRect(0, 0, w, h);
  const pad = 16;
  const left = pad + 28;
  const timelineH = Math.min(64, Math.max(32, h * 0.12));
  const spec = { x: left, y: 52, w: w - left - pad, h: Math.max(60, h - 52 - timelineH - 56) };
  const line = { x: left, y: spec.y + spec.h + 30, w: spec.w, h: timelineH };
  drawHeader(ctx, p, f, pad, w - pad * 2);
  drawGrid(ctx, p, spec);
  drawBands(ctx, p, spec, f, bands);
  if (f.thresholdDb !== null && f.thresholdDb !== undefined) drawThreshold(ctx, p, spec, f.thresholdDb);
  drawTimeline(ctx, p, line, f.history);
}

/** Resizes the canvas backing store to its CSS box at the device pixel ratio; returns CSS size. */
export function fitCanvas(canvas, ctx, dpr) {
  const w = Math.max(1, Math.round(canvas.clientWidth));
  const h = Math.max(1, Math.round(canvas.clientHeight));
  const bw = Math.round(w * dpr);
  const bh = Math.round(h * dpr);
  if (canvas.width !== bw || canvas.height !== bh) {
    canvas.width = bw;
    canvas.height = bh;
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  return { w, h };
}

/**
 * Live view analyser: log-frequency LED bands with peak hold, a dB grid, the voice band, the
 * gate threshold, and a scrolling gate timeline. Pure maths here plus one draw function; the
 * caller owns the canvas and the audio analysis.
 */

export const SCOPE = {
  bands: 64,
  fMin: 40,
  fMax: 16000,
  dbMin: -100, // AnalyserNode default minDecibels: byte 0
  dbMax: -30, // AnalyserNode default maxDecibels: byte 255
  riseRate: 30, // level units per second (fast attack)
  fallRate: 1.6,
  peakHoldS: 0.9,
  peakFallRate: 0.6,
  historyS: 6,
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

function curvePoints(box, values, bands) {
  const { x, y, w, h } = box;
  return bands.map((band, i) => ({
    x: x + freqFraction(band.centre) * w,
    y: y + h - Math.min(1, Math.max(0, values[i])) * h,
  }));
}

/** A smooth line through the points (midpoint quadratic curves). */
function tracePath(ctx, pts, join = false) {
  if (join) ctx.lineTo(pts[0].x, pts[0].y);
  else ctx.moveTo(pts[0].x, pts[0].y);
  for (let i = 1; i < pts.length - 1; i += 1) {
    const mx = (pts[i].x + pts[i + 1].x) / 2;
    const my = (pts[i].y + pts[i + 1].y) / 2;
    ctx.quadraticCurveTo(pts[i].x, pts[i].y, mx, my);
  }
  const last = pts[pts.length - 1];
  ctx.lineTo(last.x, last.y);
}

function drawSpectrum(ctx, p, box, f, bands) {
  const colour = stateColour(p, f.state);
  const pts = curvePoints(box, f.levels, bands);
  const bottom = box.y + box.h;
  ctx.beginPath();
  ctx.moveTo(pts[0].x, bottom);
  tracePath(ctx, pts, true);
  ctx.lineTo(pts[pts.length - 1].x, bottom);
  ctx.closePath();
  ctx.globalAlpha = 0.18;
  ctx.fillStyle = colour;
  ctx.fill();
  ctx.globalAlpha = 1;
  ctx.beginPath();
  tracePath(ctx, curvePoints(box, f.peaks, bands));
  ctx.strokeStyle = p.dim;
  ctx.lineWidth = 1;
  ctx.stroke();
  ctx.beginPath();
  tracePath(ctx, pts);
  ctx.strokeStyle = colour;
  ctx.lineWidth = 2;
  ctx.lineJoin = 'round';
  ctx.stroke();
}

/** Off-screen spectrogram that scrolls down one row per timeline sample. */
export function createWaterfall() {
  if (typeof document === 'undefined') return null;
  return { canvas: document.createElement('canvas'), rows: 0 };
}

function addWaterfallRow(wf, p, w, h, f, bands) {
  const c = wf.canvas;
  if (c.width !== w || c.height !== h) {
    c.width = w;
    c.height = h;
    wf.rows = 0;
  }
  const g = c.getContext('2d');
  g.drawImage(c, 0, 1);
  g.fillStyle = p.bg;
  g.fillRect(0, 0, w, 1);
  const colour = stateColour(p, f.state);
  bands.forEach((band, i) => {
    const x0 = freqFraction(band.fLo) * w;
    const x1 = freqFraction(band.fHi) * w;
    g.globalAlpha = Math.pow(Math.min(1, Math.max(0, f.levels[i])), 1.4);
    g.fillStyle = colour;
    g.fillRect(x0, 0, Math.max(1, x1 - x0), 1);
  });
  g.globalAlpha = 1;
}

function drawWaterfall(ctx, p, box, wf) {
  const { x, y, w, h } = box;
  ctx.fillStyle = p.bg;
  ctx.fillRect(x, y, w, h);
  if (wf) ctx.drawImage(wf.canvas, x, y, w, h);
  ctx.strokeStyle = p.grid;
  ctx.lineWidth = 1;
  ctx.strokeRect(x + 0.5, y + 0.5, w - 1, h - 1);
  label(ctx, p, 'now', x - 6, y + 10, 'right', p.dim, 10);
  label(ctx, p, `-${SCOPE.historyS} s`, x - 6, y + h, 'right', p.dim, 10);
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

/**
 * Draws one frame. `size` is { w, h } in CSS px (the context is already scaled for the device
 * pixel ratio). `f` = { levels, peaks, history, state, confidence, riderDb, thresholdDb|null };
 * `wf` is the waterfall from createWaterfall() and `newRow` says a timeline sample was added.
 */
export function drawScope(ctx, p, size, f, bands, wf = null, newRow = false) {
  const { w, h } = size;
  ctx.fillStyle = p.bg;
  ctx.fillRect(0, 0, w, h);
  const pad = 16;
  const left = pad + 28;
  const specW = w - left - pad;
  const avail = Math.max(120, h - 52 - 30 - 12);
  const spec = { x: left, y: 52, w: specW, h: Math.round(avail * 0.6) };
  const fall = { x: left, y: spec.y + spec.h + 30, w: specW, h: Math.max(40, avail - spec.h) };
  drawHeader(ctx, p, f, pad, w - pad * 2);
  drawGrid(ctx, p, spec);
  drawSpectrum(ctx, p, spec, f, bands);
  if (f.thresholdDb !== null && f.thresholdDb !== undefined) drawThreshold(ctx, p, spec, f.thresholdDb);
  if (wf && newRow) addWaterfallRow(wf, p, Math.max(1, Math.round(fall.w)), Math.max(1, Math.round(fall.h)), f, bands);
  drawWaterfall(ctx, p, fall, wf);
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

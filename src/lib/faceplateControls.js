/**
 * faceplateControls.js — pure logic behind the faceplate keys and knobs, kept free of React so it
 * can be tested with node:test. Every function returns new values and never mutates its inputs.
 */

// ---------- two-press keys ----------
// Destructive keys need two presses: the first arms the key, a second press inside the window acts.

export const ARM_TIMEOUT_MS = 3000;
export const IDLE = Object.freeze({ armedAt: null });

export function isArmed(state, now, timeoutMs = ARM_TIMEOUT_MS) {
  return state.armedAt !== null && now - state.armedAt < timeoutMs;
}

export function pressTwoPress(state, now, timeoutMs = ARM_TIMEOUT_MS) {
  if (isArmed(state, now, timeoutMs)) return { state: IDLE, fire: true };
  return { state: Object.freeze({ armedAt: now }), fire: false };
}

// ---------- knobs ----------
// The ring sweeps 270 degrees, from 7 o'clock to 5 o'clock. Ring radius 17 in a 40-unit box.
// A range is { min, max, step, bipolar?, detent?, taper? }. A taper maps a value onto the knob's
// turn (0..1) and back; the default is linear. Drag, wheel and typed entry live in knobTravel.js.

const RING_CIRCUMFERENCE = 2 * Math.PI * 17;
export const RING_SWEEP = RING_CIRCUMFERENCE * 0.75;
export const RING_DASH_GAP = RING_CIRCUMFERENCE;

export const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

export const LINEAR_TAPER = Object.freeze({
  toUnit: (v, { min, max }) => (v - min) / (max - min),
  fromUnit: (u, { min, max }) => min + u * (max - min),
});

/** Equal turns per octave, for frequency knobs. Needs min > 0. */
export const LOG_TAPER = Object.freeze({
  toUnit: (v, { min, max }) => Math.log(Math.max(v, min) / min) / Math.log(max / min),
  fromUnit: (u, { min, max }) => min * (max / min) ** u,
});

export const taperOf = (range) => range.taper ?? LINEAR_TAPER;

export function knobFraction(value, range) {
  if (!Number.isFinite(value) || range.max === range.min) return 0;
  return clamp(taperOf(range).toUnit(clamp(value, range.min, range.max), range), 0, 1);
}

export function clampToStep(value, { min, max, step }) {
  const snapped = step > 0 ? Math.round((value - min) / step) * step + min : value;
  const decimals = step > 0 ? Math.max(0, -Math.floor(Math.log10(step))) + 1 : 6;
  return Number(clamp(snapped, min, max).toFixed(decimals));
}

/** Arc start angle and length for the value ring, and the pointer angle. Bipolar knobs grow from 12 o'clock. */
export function knobView(value, range) {
  const n = knobFraction(value, range);
  const arcStart = range.bipolar ? 135 + Math.min(n, 0.5) * 270 : 135;
  const arcLength = range.bipolar ? Math.abs(n - 0.5) * RING_SWEEP : n * RING_SWEEP;
  return { fraction: n, arcStart, arcLength, pointerDeg: -135 + n * 270 };
}

/** New value for a key press, or null when the key does not move the knob. */
export function knobKeyStep(value, range, key, shift = false) {
  const step = range.step * (shift ? 10 : 1);
  const next = {
    ArrowUp: value + step, ArrowRight: value + step,
    ArrowDown: value - step, ArrowLeft: value - step,
    PageUp: value + step * 5, PageDown: value - step * 5,
    Home: range.min, End: range.max,
  }[key];
  return next === undefined ? null : clampToStep(next, range);
}

/** The value a double-click returns to: the default, else the detent, else the bottom stop. */
export function knobResetValue(range, defaultValue) {
  return clampToStep(defaultValue ?? range.detent ?? range.min, range);
}

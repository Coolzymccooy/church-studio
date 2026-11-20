/**
 * faceplateStrip.js — the faceplate channel strip's knobs, layout and readouts. Ranges come from
 * the mixer contract (mixerEngine.paramRange), so the knobs can never send a value the engine
 * would clamp.
 */
import { faderDbToPosition, paramRange, positionToFaderDb } from './mixerEngine.js';
import { LOG_TAPER } from './faceplateControls.js';

/** Send knobs turn like the faders: 0 dB at three quarters of the turn, so the useful top is wide. */
export const SEND_TAPER = Object.freeze({ toUnit: (db) => faderDbToPosition(db), fromUnit: (u) => positionToFaderDb(u) });

/** At most four characters so values fit under a 26px knob: one decimal below 10, whole numbers above. */
export function compactDb(v) {
  if (!Number.isFinite(v)) return '-inf';
  const tenth = Math.round(v * 10) / 10;
  const text = Math.abs(tenth) < 10 ? tenth.toFixed(1) : String(Math.round(v));
  return tenth > 0 ? `+${text}` : text;
}

export function panText(v) {
  if (Math.abs(v) < 0.02) return 'c';
  return `${v < 0 ? 'l' : 'r'}${Math.round(Math.abs(v) * 100)}`;
}

export function sendText(db) {
  return db <= -90 ? '-inf' : compactDb(db);
}

const knob = (param, label, job, step, defaultValue, fmt, extra = {}) => {
  const [min, max] = paramRange(param);
  return { param, label, job, min, max, step, defaultValue, fmt, bipolar: false, off: () => false, ...extra };
};

/** Knob id → definition. `off(strip)` greys the knob when its stage is switched off. */
export const STRIP_KNOBS = {
  trim: knob('trim_db', 'trim', 'trim', 0.5, 0, (v) => compactDb(v), { detent: 0 }),
  hz: knob('hpf_hz', 'hz', 'hpf', 5, 80, (v) => String(Math.round(v)), { taper: LOG_TAPER, off: (s) => !s.hpf_enabled }),
  gthr: knob('gate_threshold_db', 'g thr', 'dyn', 1, -45, (v) => String(Math.round(v)), { off: (s) => !s.gate_enabled }),
  cthr: knob('comp_threshold_db', 'c thr', 'dyn', 1, -18, (v) => String(Math.round(v)), { off: (s) => !s.comp_enabled }),
  ratio: knob('comp_ratio', 'ratio', 'dyn', 0.5, 3, (v) => (Number.isInteger(v) ? String(v) : v.toFixed(1)), { off: (s) => !s.comp_enabled }),
  hi: knob('eq_high_db', 'hi', 'eq', 0.5, 0, (v) => compactDb(v), { bipolar: true, detent: 0 }),
  mid: knob('eq_mid_db', 'mid', 'eq', 0.5, 0, (v) => compactDb(v), { bipolar: true, detent: 0 }),
  lo: knob('eq_low_db', 'lo', 'eq', 0.5, 0, (v) => compactDb(v), { bipolar: true, detent: 0 }),
  main: knob('send_main_db', 'main', 'send', 0.5, 0, sendText, { taper: SEND_TAPER, detent: 0 }),
  strm: knob('send_stream_db', 'strm', 'send', 0.5, 0, sendText, { taper: SEND_TAPER, detent: 0 }),
  mon: knob('send_monitor_db', 'mon', 'send', 0.5, 0, sendText, { taper: SEND_TAPER, detent: 0 }),
  pan: knob('pan', 'pan', 'pan', 0.05, 0, panText, { bipolar: true, detent: 0 }),
};

/** Knob rows, top to bottom; the toggle keys sit between them in the component. */
/** Duck depth: how far a music strip dips while someone on an auto-mix strip speaks. */
export const DUCK_KNOB = knob('duck_depth_db', 'duck', 'dyn', 1, 12, (v) => String(Math.round(v)));

export const STRIP_ROWS = [['trim', 'hz', 'pan'], ['gthr', 'cthr', 'ratio'], ['hi', 'mid', 'lo'], ['main', 'strm', 'mon']];

export const STRIP_COLOURS = ['amber', 'clay', 'teal', 'olive', 'sky', 'rose', 'stone'];

/** Scribble-strip colour by position, until strips carry their own colour. */
export function stripColour(index) {
  return STRIP_COLOURS[index % STRIP_COLOURS.length];
}

/** The engine reports gate_open; undefined means no reading yet, shown as open like the old dot. */
export function stripGateTone(gateEnabled, gateOpen) {
  if (!gateEnabled) return 'off';
  return gateOpen === false ? 'a' : 'g';
}

/** "mix -3.2" / "duck -12": a reduction readout that always fits the strip, like grText. */
export function reductionText(prefix, db) {
  // At most eight characters: a four-letter prefix ("duck") reads in whole dB.
  const decimals = prefix.length <= 3;
  if (!Number.isFinite(db) || db < 0.1) return decimals ? `${prefix} 0.0` : `${prefix} 0`;
  if (decimals && db < 9.95) return `${prefix} -${db.toFixed(1)}`;
  return `${prefix} -${Math.min(99, Math.max(1, Math.round(db)))}`;
}

/** True when the engine reports auto-mix and ducking for this strip (newer engines only). */
export function hasAutomix(strip) {
  return Boolean(strip) && 'automix' in strip && 'duck' in strip;
}

export function grText(grDb) {
  if (!Number.isFinite(grDb) || grDb < 0.1) return 'gr 0.0';
  // Ten dB and more drop the decimal, so the reading always fits the strip ("gr -12").
  return grDb < 9.95 ? `gr -${grDb.toFixed(1)}` : `gr -${Math.min(99, Math.round(grDb))}`;
}

// ---------- buses ----------

/** The bus limiter ceiling knob. The limiter itself is always on; only its ceiling moves. */
export const BUS_LIMITER = knob('limiter_ceiling_db', 'lim', 'dyn', 0.5, -1, (v) => compactDb(v));

const BUS_LABELS = { main: 'main', stream: 'stream', monitor: 'mon' };

export function busLabel(id) {
  return BUS_LABELS[id] ?? String(id);
}

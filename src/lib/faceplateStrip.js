/**
 * faceplateStrip.js — the faceplate channel strip's knobs, layout and readouts. Ranges come from
 * the mixer contract (mixerEngine.paramRange), so the knobs can never send a value the engine
 * would clamp.
 */
import { paramRange } from './mixerEngine.js';

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
  trim: knob('trim_db', 'trim', 'trim', 0.5, 0, (v) => compactDb(v)),
  hz: knob('hpf_hz', 'hz', 'hpf', 5, 80, (v) => String(Math.round(v)), { off: (s) => !s.hpf_enabled }),
  gthr: knob('gate_threshold_db', 'g thr', 'dyn', 1, -45, (v) => String(Math.round(v)), { off: (s) => !s.gate_enabled }),
  cthr: knob('comp_threshold_db', 'c thr', 'dyn', 1, -18, (v) => String(Math.round(v)), { off: (s) => !s.comp_enabled }),
  ratio: knob('comp_ratio', 'ratio', 'dyn', 0.5, 3, (v) => (Number.isInteger(v) ? String(v) : v.toFixed(1)), { off: (s) => !s.comp_enabled }),
  hi: knob('eq_high_db', 'hi', 'eq', 0.5, 0, (v) => compactDb(v), { bipolar: true }),
  mid: knob('eq_mid_db', 'mid', 'eq', 0.5, 0, (v) => compactDb(v), { bipolar: true }),
  lo: knob('eq_low_db', 'lo', 'eq', 0.5, 0, (v) => compactDb(v), { bipolar: true }),
  main: knob('send_main_db', 'main', 'send', 0.5, 0, sendText),
  strm: knob('send_stream_db', 'strm', 'send', 0.5, 0, sendText),
  mon: knob('send_monitor_db', 'mon', 'send', 0.5, 0, sendText),
  pan: knob('pan', 'pan', 'pan', 0.05, 0, panText, { bipolar: true }),
};

/** Knob rows, top to bottom; the toggle keys sit between them in the component. */
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

export function grText(grDb) {
  return Number.isFinite(grDb) && grDb >= 0.1 ? `gr -${grDb.toFixed(1)}` : 'gr 0.0';
}

// ---------- buses ----------

/** The bus limiter ceiling knob. The limiter itself is always on; only its ceiling moves. */
export const BUS_LIMITER = knob('limiter_ceiling_db', 'lim', 'dyn', 0.5, -1, (v) => compactDb(v));

const BUS_LABELS = { main: 'main', stream: 'stream', monitor: 'mon' };

export function busLabel(id) {
  return BUS_LABELS[id] ?? String(id);
}

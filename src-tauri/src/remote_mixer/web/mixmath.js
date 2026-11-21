// Pure maths for the remote mixer page: fader taper and detents, fine-adjust steps, meter
// ballistics (the desktop ladder's numbers), clip latching and readout formatting.

export const DB_OFF = -90;
export const DB_MAX = 10;
export const METER_MIN_DB = -60;
export const METER_MAX_DB = 6;
export const ZONE_AMBER_DB = -18;
export const ZONE_RED_DB = -3;
export const CLIP_DB = -0.1;
export const ZERO_DETENT_DB = 0.5;

const MINUS = '−';
const CLIP_LATCH_MS = 2000;
const OFF_ZONE = 0.01;
const LOWEST_STEP_DB = -60;
const RISE_RATE = 22;
const FALL_RATE = 7;
const PEAK_HOLD_SECONDS = 1;
const PEAK_DECAY_DB_PER_SEC = 20;
// Within this of the floor a meter is at the floor, so a silent meter settles exactly.
const FLOOR_SNAP_DB = 0.05;

// A console-style taper: unity three quarters up, the useful range spread out, off at the bottom.
const TAPER = [
  [DB_OFF, 0], [-60, 0.06], [-50, 0.12], [-40, 0.2], [-30, 0.3], [-20, 0.43],
  [-10, 0.6], [-5, 0.7], [0, 0.8], [5, 0.9], [DB_MAX, 1],
];

const round1 = (value) => Math.round(value * 10) / 10;

function interpolate(value, from, to) {
  for (let i = 1; i < TAPER.length; i += 1) {
    const [a0, b0] = [TAPER[i - 1][from], TAPER[i][from]];
    if (value <= b0) {
      const [a1, b1] = [TAPER[i - 1][to], TAPER[i][to]];
      return a1 + ((value - a0) / (b0 - a0)) * (b1 - a1);
    }
  }
  return TAPER[TAPER.length - 1][to];
}

/** Fader travel (0 bottom, 1 top) for a send or fader level. */
export function faderPosition(db) {
  if (!Number.isFinite(db) || db <= DB_OFF) return 0;
  if (db >= DB_MAX) return 1;
  return interpolate(db, 0, 1);
}

/** The level at a fader position, to 0.1 dB; the bottom of the travel is off. */
export function faderDb(position) {
  if (!(position > OFF_ZONE)) return DB_OFF;
  if (position >= 1) return DB_MAX;
  return round1(interpolate(position, 1, 0));
}

/** Snap to unity when a drag passes close to it. */
export function withDetent(db) {
  return db > DB_OFF && Math.abs(db) < ZERO_DETENT_DB ? 0 : db;
}

export function clampDb(db) {
  const value = round1(Math.min(DB_MAX, db));
  return value <= DB_OFF ? DB_OFF : value;
}

/** One fine-adjust step. Up from off starts at -60 dB; down past the bottom is off. */
export function nudge(db, step) {
  if (db <= DB_OFF) return step > 0 ? LOWEST_STEP_DB : DB_OFF;
  return clampDb(db + step);
}

/** What a channel contributes to the mix: its send tap plus its send level. */
export function inMixDb(tapDb, sendDb) {
  if (!Number.isFinite(tapDb) || !(sendDb > DB_OFF)) return -Infinity;
  return tapDb + sendDb;
}

export function meterFraction(db) {
  if (!Number.isFinite(db)) return db === Infinity ? 1 : 0;
  const clamped = Math.max(METER_MIN_DB, Math.min(METER_MAX_DB, db));
  return (clamped - METER_MIN_DB) / (METER_MAX_DB - METER_MIN_DB);
}

/** 'g' safe, 'a' caution, 'r' too hot. */
export function meterZone(db) {
  if (db >= ZONE_RED_DB) return 'r';
  if (db >= ZONE_AMBER_DB) return 'a';
  return 'g';
}

export function initialMeter() {
  return { level: METER_MIN_DB, peak: METER_MIN_DB, hold: 0 };
}

/** Advance a meter by dt seconds toward targetDb. `instant` skips the ballistics (reduced motion). */
export function stepMeter(prev, targetDb, dt, { instant = false } = {}) {
  const target = Number.isFinite(targetDb) ? Math.max(METER_MIN_DB, targetDb) : METER_MIN_DB;
  const rate = target > prev.level ? RISE_RATE : FALL_RATE;
  const eased = instant ? target : prev.level + (target - prev.level) * Math.min(1, rate * dt);
  const level = eased - METER_MIN_DB < FLOOR_SNAP_DB ? METER_MIN_DB : eased;
  if (level >= prev.peak) return { level, peak: level, hold: PEAK_HOLD_SECONDS };
  const hold = prev.hold - dt;
  const peak = hold > 0 ? prev.peak : Math.max(METER_MIN_DB, prev.peak - dt * PEAK_DECAY_DB_PER_SEC);
  return { level, peak, hold };
}

/** When a clip lamp lit by `peakDb` at `now` (ms) goes out. */
export function clipUntil(until, peakDb, now) {
  return peakDb >= CLIP_DB ? now + CLIP_LATCH_MS : until;
}

/** Pinned channels first, each group in its original order. */
export function orderStrips(items, pinned) {
  return [...items.filter(({ name }) => pinned.has(name)), ...items.filter(({ name }) => !pinned.has(name))];
}

function signed(db) {
  const text = Math.abs(db).toFixed(1);
  if (db < 0 && text !== '0.0') return `${MINUS}${text}`;
  return db > 0 && text !== '0.0' ? `+${text}` : text;
}

/** A fixed five-character readout, so digits never shift as the value changes. */
export function lcdReadout(db) {
  return (db <= DB_OFF ? 'Off' : signed(db)).padStart(5, ' ');
}

/** A peak-hold readout: minus infinity below the meter floor. */
export function peakLabel(db) {
  return db <= METER_MIN_DB ? `${MINUS}∞` : signed(db);
}

/** The widest strip that fits `count` strips in `available` px, kept within [min, max]. */
export function fitStrip({ available, count, gap, min, max }) {
  if (!(count > 0)) return max;
  const each = Math.floor((available - gap * (count - 1)) / count);
  return Math.max(min, Math.min(max, each));
}

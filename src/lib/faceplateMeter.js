/**
 * faceplateMeter.js — ballistics and scale for the faceplate meter ladder. Pure, so the canvas
 * renderer stays a thin drawing loop and the maths is testable.
 */

export const METER_MIN_DB = -60;
export const METER_MAX_DB = 6;
export const ZONE_AMBER_DB = -18;
export const ZONE_RED_DB = -3;

const RISE_RATE = 22;
const FALL_RATE = 7;
const PEAK_HOLD_SECONDS = 1;
const PEAK_DECAY_DB_PER_SEC = 20;

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

/** Advance the meter by dt seconds toward targetDb. */
export function stepMeter(prev, targetDb, dt) {
  const target = Number.isFinite(targetDb) ? targetDb : METER_MIN_DB;
  const rate = target > prev.level ? RISE_RATE : FALL_RATE;
  const level = prev.level + (target - prev.level) * Math.min(1, rate * dt);
  if (level >= prev.peak) return { level, peak: level, hold: PEAK_HOLD_SECONDS };
  const hold = prev.hold - dt;
  const peak = hold > 0 ? prev.peak : Math.max(METER_MIN_DB, prev.peak - dt * PEAK_DECAY_DB_PER_SEC);
  return { level, peak, hold };
}

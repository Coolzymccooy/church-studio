/** Loudness display scale and helpers (values in LUFS; LU for differences). */
export const LOUD_MIN = -40;
export const LOUD_MAX = 0;
/** Tolerance around the target that counts as "on target", in LU. */
export const ON_TARGET_LU = 1;
/** Above target by this much is a fault (too loud for the platform). */
export const TOO_LOUD_LU = 3;
/** Seconds of short-term loudness kept for the history graph. */
export const HISTORY_S = 60;

const isNum = (v) => typeof v === 'number' && Number.isFinite(v);

/** Position 0..1 of a loudness value on the display scale (clamped; null → 0). */
export function loudFraction(lufs) {
  if (!isNum(lufs)) return 0;
  return Math.min(1, Math.max(0, (lufs - LOUD_MIN) / (LOUD_MAX - LOUD_MIN)));
}

/**
 * Distance from target, as words and a tone: 'good' within ±1 LU, 'fault' 3 LU or more over,
 * 'caution' otherwise over, 'low' under. Null input gives no reading.
 */
export function targetDelta(lufs, target) {
  if (!isNum(lufs) || !isNum(target)) return { lu: null, text: 'no signal', tone: 'idle' };
  const lu = Math.round((lufs - target) * 10) / 10;
  if (Math.abs(lu) <= ON_TARGET_LU) return { lu, text: 'on target', tone: 'good' };
  const text = `${lu > 0 ? '+' : ''}${lu.toFixed(1)} LU`;
  if (lu >= TOO_LOUD_LU) return { lu, text, tone: 'fault' };
  if (lu > 0) return { lu, text, tone: 'caution' };
  return { lu, text, tone: 'low' };
}

/** A new history with `value` appended (null kept as a gap), at most `max` entries. */
export function pushHistory(history, value, max) {
  const next = [...history, isNum(value) ? value : null];
  return next.length > max ? next.slice(next.length - max) : next;
}

/** "-21.9" style text for a loudness value, or "--". */
export function lufsText(v) {
  return isNum(v) ? v.toFixed(1) : '--';
}

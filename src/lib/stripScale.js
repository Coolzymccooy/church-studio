/** Mixer strip size: "fit" scales the strips to the height available; the others are fixed. */
export const STRIP_SIZES = ['fit', '1', '1.25', '1.5'];
/** Height in CSS px that a strip row needs at scale 1 (strip plus the row padding). */
export const STRIP_ROW_PX = 590;
/** Extra height for the optional strip rows: the auto/duck keys, the duck knob, the readout. */
export const EXTRA_ROW_PX = { keys: 28, knob: 52, readout: 38 };

/** Height the tallest strip needs, given which optional rows any strip shows. */
export function stripRowPx(strips, extraMonitorRows = 0) {
  const list = Array.isArray(strips) ? strips : [];
  const has = (pred) => list.some(pred);
  let px = STRIP_ROW_PX;
  if (has((s) => 'automix' in s && 'duck' in s)) px += EXTRA_ROW_PX.keys;
  if (has((s) => 'group_membership' in s)) px += EXTRA_ROW_PX.keys;
  if (has((s) => s.duck)) px += EXTRA_ROW_PX.knob;
  if (has((s) => s.automix || s.duck)) px += EXTRA_ROW_PX.readout;
  if (extraMonitorRows > 0) px += EXTRA_ROW_PX.knob;
  return px;
}
export const FIT_MIN = 0.75;
export const FIT_MAX = 1.6;

export function readStripSize(value) {
  return STRIP_SIZES.includes(value) ? value : 'fit';
}

export function nextStripSize(current) {
  const i = STRIP_SIZES.indexOf(readStripSize(current));
  return STRIP_SIZES[(i + 1) % STRIP_SIZES.length];
}

/**
 * The scale for a strip area of `heightPx`, rounded down to 0.05 so it never overflows; it may
 * shrink to FIT_MIN on a short window so the faders stay in view.
 */
export function fitScale(heightPx, rowPx = STRIP_ROW_PX) {
  // Height not measured yet: show the strips at their own size.
  if (!Number.isFinite(heightPx) || heightPx <= 0) return 1;
  const raw = Math.floor((heightPx / rowPx) * 20) / 20;
  return Math.min(FIT_MAX, Math.max(FIT_MIN, raw));
}

export function stripScale(size, heightPx, rowPx = STRIP_ROW_PX) {
  const s = readStripSize(size);
  return s === 'fit' ? fitScale(heightPx, rowPx) : Number(s);
}

export function stripSizeLabel(size) {
  const s = readStripSize(size);
  return s === 'fit' ? 'size fit' : `size ${s}×`;
}

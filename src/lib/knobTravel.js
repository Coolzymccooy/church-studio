/**
 * knobTravel.js — how a faceplate knob turns: drag, wheel and trackpad, fine mode, the detent and
 * typed entry. Pure functions, tested with node:test; Knob.jsx only wires events to them.
 *
 * Drags and scrolls move a position along the knob's travel (0..span pixels) and the value is read
 * back from that position. Keeping the position, not the value, lets tiny trackpad deltas add up,
 * lets fine mode change mid-gesture, and gives the detent a band the knob rests in.
 */
import { clamp, clampToStep, knobFraction, taperOf } from './faceplateControls.js';

/** Pixels of drag for the whole range, and the extra travel the knob rests on at its detent. */
export const DRAG_THROW_PX = 200;
export const DETENT_PX = 10;
/** Shift or Alt held: a drag or scroll moves the knob a fifth as far. */
export const FINE_FACTOR = 0.2;
/** Trackpad pixels → travel pixels. A notched wheel click → travel pixels (40 clicks for the range). */
export const TRACKPAD_GAIN = 0.4;
export const WHEEL_NOTCH_PX = 5;
/** A scroll that began on the page keeps scrolling the page for this long when it passes a knob. */
export const PAGE_SCROLL_HOLD_MS = 300;

const hasDetent = (range) => Number.isFinite(range.detent) && range.detent > range.min && range.detent < range.max;
const detentAt = (range) => knobFraction(range.detent, range) * DRAG_THROW_PX;

export function knobTravelSpan(range) {
  return DRAG_THROW_PX + (hasDetent(range) ? DETENT_PX : 0);
}

/** Where a value sits on the travel. A value on the detent sits in the middle of its band. */
export function knobTravelFor(value, range) {
  const at = knobFraction(value, range) * DRAG_THROW_PX;
  if (!hasDetent(range)) return at;
  const d = detentAt(range);
  if (clampToStep(value, range) === clampToStep(range.detent, range)) return d + DETENT_PX / 2;
  return at < d ? at : at + DETENT_PX;
}

/** The value at a point on the travel, snapped to the step and the range. */
export function knobValueAt(travel, range) {
  const t = clamp(travel, 0, knobTravelSpan(range));
  let unit = t / DRAG_THROW_PX;
  if (hasDetent(range)) {
    const d = detentAt(range);
    if (t >= d && t <= d + DETENT_PX) return clampToStep(range.detent, range);
    unit = (t > d ? t - DETENT_PX : t) / DRAG_THROW_PX;
  }
  return clampToStep(taperOf(range).fromUnit(clamp(unit, 0, 1), range), range);
}

/** Travel after a pointer move of (dx, dy) pixels: up or right turns the knob up, whichever is larger. */
export function knobDragTravel(travel, dx, dy, range, fine = false) {
  const delta = Math.abs(dy) >= Math.abs(dx) ? -dy : dx;
  return clamp(travel + delta * (fine ? FINE_FACTOR : 1), 0, knobTravelSpan(range));
}

/** Value after a vertical drag of dy pixels (negative is up) from where the drag started. */
export function knobDragValue(startValue, dy, range, fine = false) {
  return knobValueAt(knobDragTravel(knobTravelFor(startValue, range), 0, dy, range, fine), range);
}

// A notched mouse wheel reports whole multiples of about 100 px a click; a trackpad reports
// small, often fractional deltas.
const isNotchedWheel = (delta) => Math.abs(delta) >= 50 && Number.isInteger(delta);

/**
 * Travel pixels for one wheel event, or null when the event is not for the knob: a sideways
 * scroll (the strip bank keeps it) or a pinch. Fingers or wheel pushed up turn the knob up,
 * whichever way the system scrolls: `inverted` is WebKit's webkitDirectionInvertedFromDevice
 * (macOS natural scrolling). Shift+wheel arrives as deltaX on some systems, so it reads as vertical.
 */
export function knobWheelTravel({ deltaX = 0, deltaY = 0, deltaMode = 0, inverted = false, shiftKey = false, ctrlKey = false }) {
  if (ctrlKey) return null;
  const swapped = shiftKey && deltaY === 0;
  const vertical = swapped ? deltaX : deltaY;
  const sideways = swapped ? 0 : deltaX;
  if (vertical === 0 || Math.abs(sideways) > Math.abs(vertical)) return null;
  const up = inverted ? vertical : -vertical;
  if (deltaMode === 1) return (up / 3) * WHEEL_NOTCH_PX;
  if (deltaMode === 2) return up * WHEEL_NOTCH_PX * 4;
  if (isNotchedWheel(up)) return Math.sign(up) * Math.max(1, Math.round(Math.abs(up) / 100)) * WHEEL_NOTCH_PX;
  return up * TRACKPAD_GAIN;
}

/** Travel after a wheel event; null when the wheel is not for the knob. */
export function knobWheelStep(travel, wheel, range, fine = false) {
  const delta = knobWheelTravel(wheel);
  if (delta === null) return null;
  return clamp(travel + delta * (fine ? FINE_FACTOR : 1), 0, knobTravelSpan(range));
}

/** False while a scroll that began on the page is still passing over the knob. */
export function knobTakesWheel(now, lastPageWheelAt) {
  return now - lastPageWheelAt >= PAGE_SCROLL_HOLD_MS;
}

/**
 * A typed value: a number (units and a leading + ignored), "-inf" for the bottom stop, and on a
 * bipolar knob "c" for centre or "l30" / "r30" for percent left or right. Null when unreadable.
 */
export function parseKnobText(text, range) {
  const t = String(text ?? '').trim().toLowerCase().replace(/\s+/g, '');
  if (t === '') return null;
  if (t === '-inf' || t === 'inf' || t === '-∞') return range.min;
  const mid = (range.min + range.max) / 2;
  if (range.bipolar && t === 'c') return clampToStep(mid, range);
  const side = range.bipolar ? /^([lr])(\d+(?:\.\d+)?)%?$/.exec(t) : null;
  if (side) {
    const share = (Number(side[2]) / 100) * ((range.max - range.min) / 2);
    return clampToStep(mid + (side[1] === 'l' ? -share : share), range);
  }
  const num = /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)/.exec(t);
  return num ? clampToStep(Number(num[0]), range) : null;
}

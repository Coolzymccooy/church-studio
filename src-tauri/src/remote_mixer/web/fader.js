// A thumb fader. The drag is relative (the cap moves from where it is, never jumps to the
// finger), so a stray touch cannot slam a send. Vertical drags move the fader; sideways
// swipes are left to the browser so the strip bank scrolls. Unity has a magnetic detent.
import { DB_MAX, DB_OFF, faderDb, faderPosition, nudge, withDetent } from './mixmath.js';
import { dbLabel } from './protocol.js';

export const TICKS = [10, 5, 0, -5, -10, -20, -30, -40, -60];
const INTENT_PX = 6;
const DOUBLE_TAP_MS = 320;
const KEY_STEPS = { ArrowUp: 0.5, ArrowRight: 0.5, ArrowDown: -0.5, ArrowLeft: -0.5, PageUp: 3, PageDown: -3 };

function span(className) {
  const el = document.createElement('span');
  el.className = className;
  return el;
}

/** Slot, printed ticks and cap inside `track`. Returns the cap. */
export function buildTrack(track) {
  track.append(span('slot'));
  for (const db of TICKS) {
    const tick = span(db === 0 ? 'tick unity' : 'tick');
    tick.style.setProperty('--p', faderPosition(db).toFixed(4));
    track.append(tick);
  }
  const cap = span('cap');
  track.append(cap);
  return cap;
}

/** Print the scale numbers once, beside the first strip. */
export function buildScale(scale) {
  scale.replaceChildren(...TICKS.map((db) => {
    const label = document.createElement('i');
    label.textContent = db > 0 ? `+${db}` : String(db).replace('-', '−');
    if (db === 0) label.className = 'unity';
    label.style.setProperty('--p', faderPosition(db).toFixed(4));
    return label;
  }));
}

/** Show `db` on a track without sending anything. */
export function showLevel(track, db) {
  track.style.setProperty('--pos', faderPosition(db).toFixed(4));
  track.classList.toggle('detent', db === 0);
  track.setAttribute('aria-valuenow', String(db));
  track.setAttribute('aria-valuetext', dbLabel(db));
}

function buzz() {
  try { navigator.vibrate?.(8); } catch { /* optional on this device */ }
}

/**
 * Wire pointer and keyboard input. `get()` reads the shown level; `set(db)` is called for every
 * change; `grab(true|false)` brackets a drag; `select()` runs on any touch.
 */
export function attachFader(track, { get, set, grab, select, enabled }) {
  let drag = null;
  let lastTap = 0;

  const change = (db) => {
    if (db === get()) return;
    if (db === 0) buzz();
    set(db);
  };

  const end = (event) => {
    if (!drag || event.pointerId !== drag.id) return;
    const { moved } = drag;
    drag = null;
    track.classList.remove('grab');
    grab(false);
    if (moved || event.type !== 'pointerup') return;
    const now = performance.now();
    // Double-tap returns a live fader to unity; it never opens a send that is off.
    if (now - lastTap < DOUBLE_TAP_MS && get() > DB_OFF) {
      lastTap = 0;
      change(0);
    } else {
      lastTap = now;
    }
  };

  track.addEventListener('pointerdown', (event) => {
    if (!enabled() || event.button !== 0 || drag) return;
    select();
    // Hold from touch-down, so a snapshot arriving before the drag starts cannot move the cap
    // under the finger and then be undone by the drag.
    grab(true);
    drag = { id: event.pointerId, x: event.clientX, y: event.clientY, start: faderPosition(get()), moved: false };
  });

  track.addEventListener('pointermove', (event) => {
    if (!drag || event.pointerId !== drag.id) return;
    const dy = event.clientY - drag.y;
    if (!drag.moved) {
      const dx = event.clientX - drag.x;
      if (Math.abs(dy) < INTENT_PX) return;
      if (Math.abs(dx) > Math.abs(dy)) {
        drag = null;
        grab(false);
        return;
      }
      drag.moved = true;
      track.setPointerCapture?.(event.pointerId);
      track.classList.add('grab');
    }
    const cap = track.querySelector('.cap');
    const travel = Math.max(1, track.clientHeight - (cap?.offsetHeight ?? 0));
    const position = Math.min(1, Math.max(0, drag.start - dy / travel));
    change(withDetent(faderDb(position)));
  });

  for (const type of ['pointerup', 'pointercancel', 'lostpointercapture']) track.addEventListener(type, end);

  track.addEventListener('keydown', (event) => {
    if (!enabled()) return;
    let next = null;
    if (event.key in KEY_STEPS) next = nudge(get(), KEY_STEPS[event.key] * (event.shiftKey ? 0.2 : 1));
    else if (event.key === 'Home') next = DB_OFF;
    else if (event.key === 'End') next = DB_MAX;
    else if (event.key === '0') next = 0;
    if (next === null) return;
    event.preventDefault();
    select();
    change(next);
  });
}

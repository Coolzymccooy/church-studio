import { useCallback, useEffect, useRef, useState } from 'react';
import { knobDragTravel, knobTakesWheel, knobTravelFor, knobValueAt, knobWheelStep } from '../../lib/knobTravel.js';

/** How long the value readout stays lit after the last scroll. */
const LIVE_HOLD_MS = 700;

// A scroll that began on the page stays a page scroll while it passes over knobs, until it pauses.
let lastPageWheelAt = -Infinity;
let watchingPage = false;
function watchPageWheel() {
  if (watchingPage || typeof window === 'undefined') return;
  watchingPage = true;
  window.addEventListener('wheel', (e) => {
    const onKnob = e.target instanceof Element && e.target.closest('.fp-knob');
    if (!onKnob) lastPageWheelAt = e.timeStamp;
  }, { capture: true, passive: true });
}

const isFine = (e) => e.shiftKey || e.altKey;

/**
 * Pointer drag (with pointer capture) and wheel/trackpad turning for one knob. The knob keeps a
 * position along its travel so small deltas add up; it re-reads that position from `value`
 * whenever the value changes from outside (a scene load, another control).
 * Returns the ref for the knob element, its pointer handlers, and `live` while it is being turned.
 */
export default function useKnobInput({ value, range, disabled, onSet }) {
  const elRef = useRef(null);
  const latest = useRef({ value, range, disabled, onSet });
  const pos = useRef({ value, travel: knobTravelFor(value, range) });
  const drag = useRef(null);
  const liveTimer = useRef(null);
  const [live, setLive] = useState(false);

  useEffect(() => {
    latest.current = { value, range, disabled, onSet };
    if (value !== pos.current.value) pos.current = { value, travel: knobTravelFor(value, range) };
  });

  const moveTo = useCallback((travel) => {
    const next = knobValueAt(travel, latest.current.range);
    const changed = next !== pos.current.value;
    pos.current = { value: next, travel };
    if (changed) latest.current.onSet(next);
  }, []);

  const showLive = useCallback((hold) => {
    clearTimeout(liveTimer.current);
    setLive(true);
    if (hold) liveTimer.current = setTimeout(() => setLive(false), LIVE_HOLD_MS);
  }, []);

  useEffect(() => {
    watchPageWheel();
    const el = elRef.current;
    if (!el) return undefined;
    const onWheel = (e) => {
      if (latest.current.disabled) return;
      if (!knobTakesWheel(e.timeStamp, lastPageWheelAt)) {
        lastPageWheelAt = e.timeStamp;
        return;
      }
      const wheel = {
        deltaX: e.deltaX, deltaY: e.deltaY, deltaMode: e.deltaMode,
        inverted: Boolean(e.webkitDirectionInvertedFromDevice), shiftKey: e.shiftKey, ctrlKey: e.ctrlKey,
      };
      const travel = knobWheelStep(pos.current.travel, wheel, latest.current.range, isFine(e));
      if (travel === null) return;
      e.preventDefault();
      showLive(true);
      moveTo(travel);
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [moveTo, showLive]);

  useEffect(() => () => clearTimeout(liveTimer.current), []);

  const onPointerDown = (e) => {
    if (disabled || e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    if (value !== pos.current.value) pos.current = { value, travel: knobTravelFor(value, range) };
    drag.current = { x: e.clientX, y: e.clientY };
    showLive(false);
  };
  const onPointerMove = (e) => {
    if (!drag.current) return;
    const dx = e.clientX - drag.current.x;
    const dy = e.clientY - drag.current.y;
    if (dx === 0 && dy === 0) return;
    drag.current = { x: e.clientX, y: e.clientY };
    moveTo(knobDragTravel(pos.current.travel, dx, dy, range, isFine(e)));
  };
  const endDrag = () => {
    if (!drag.current) return;
    drag.current = null;
    setLive(false);
  };

  return {
    elRef,
    live,
    handlers: { onPointerDown, onPointerMove, onPointerUp: endDrag, onPointerCancel: endDrag, onLostPointerCapture: endDrag },
  };
}

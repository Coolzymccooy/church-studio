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

  // While a gesture runs the knob is the source of the value: a render can still carry a value it
  // sent a moment ago, and re-reading the position from that would make the knob jump back.
  const turning = () => drag.current !== null || liveTimer.current !== null;
  const resync = () => {
    const { value: v, range: r } = latest.current;
    if (v !== pos.current.value) pos.current = { value: v, travel: knobTravelFor(v, r) };
  };

  useEffect(() => {
    latest.current = { value, range, disabled, onSet };
    if (!turning()) resync();
  });

  const moveTo = useCallback((travel) => {
    const next = knobValueAt(travel, latest.current.range);
    const changed = next !== pos.current.value;
    pos.current = { value: next, travel };
    if (changed) latest.current.onSet(next);
  }, []);

  const showLive = useCallback((hold) => {
    clearTimeout(liveTimer.current);
    liveTimer.current = null;
    setLive(true);
    if (hold) {
      liveTimer.current = setTimeout(() => {
        liveTimer.current = null;
        setLive(false);
      }, LIVE_HOLD_MS);
    }
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
      if (knobWheelStep(0, wheel, latest.current.range) === null) return;
      if (!turning()) resync();
      e.preventDefault();
      showLive(true);
      moveTo(knobWheelStep(pos.current.travel, wheel, latest.current.range, isFine(e)));
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [moveTo, showLive]);

  useEffect(() => () => clearTimeout(liveTimer.current), []);

  const onPointerDown = (e) => {
    if (disabled || e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    showLive(false);
    resync();
    drag.current = { x: e.clientX, y: e.clientY };
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

import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react';

import { tipPosition } from '../../lib/tipPosition.js';
import Key from './Key.jsx';
import useTwoPress from './useTwoPress.js';

/**
 * A key that needs two presses: engine stop, scene load or delete, stream, record stop, reset.
 * The first press arms it (amber edge, or red for `tone="red"`) and the legend asks ("stop?") with a
 * bar running down the 3 s window; it disarms after 3 s, on Escape or when focus leaves.
 */
/**
 * What an armed key shows, so the second press is obvious on the key itself: `armedChildren` when
 * given, otherwise the plain legend with a question mark ("stop?").
 */
function armedLegend(children, label, armedChildren) {
  if (armedChildren !== undefined) return armedChildren;
  const legend = children ?? label;
  return typeof legend === 'string' ? `${legend}?` : legend;
}

const HOVER_DELAY_MS = 400;
/** The tip every two-press key carries unless it is given its own. */
export const TWO_PRESS_TIP = 'Press twice within 3 seconds. The first press only arms it, so a stray click does nothing.';

/** Hover (after a short delay) or keyboard focus shows the tip below the key, placed on screen. */
function useKeyTip() {
  const [open, setOpen] = useState(false);
  const timer = useRef(null);
  useEffect(() => () => clearTimeout(timer.current), []);
  return {
    open,
    hoverOn: () => { clearTimeout(timer.current); timer.current = setTimeout(() => setOpen(true), HOVER_DELAY_MS); },
    show: () => { clearTimeout(timer.current); setOpen(true); },
    hide: () => { clearTimeout(timer.current); setOpen(false); },
  };
}

function KeyTipBox({ id, text, wrapRef }) {
  const box = useRef(null);
  useLayoutEffect(() => {
    const el = box.current;
    if (!el || !wrapRef.current) return;
    const { top, left } = tipPosition(wrapRef.current.getBoundingClientRect(), window.innerWidth, el.offsetWidth);
    Object.assign(el.style, { position: 'fixed', top: `${top}px`, left: `${left}px`, transform: 'none' });
  }, [wrapRef]);
  return <span ref={box} id={id} role="tooltip" className="fp-tipbox">{text}</span>;
}

export default function TwoPressKey({
  onConfirm, tone = 'amber', label, children, armedChildren, lit = null, disabled, className = '', tip = TWO_PRESS_TIP,
}) {
  const tipId = useId();
  const wrapRef = useRef(null);
  const keyTip = useKeyTip();
  const { armed, press, cancel } = useTwoPress(onConfirm);
  const armCls = tone === 'red' ? 'fp-arm-r' : 'fp-arm';
  const onKeyDown = (e) => {
    if (e.key === 'Escape' && armed) {
      e.stopPropagation();
      cancel();
    }
  };
  return (
    <span
      ref={wrapRef}
      className="fp-tipwrap"
      onPointerEnter={keyTip.hoverOn}
      onPointerLeave={keyTip.hide}
      onFocus={keyTip.show}
      onBlur={keyTip.hide}
    >
      <Key
        lit={armed ? null : lit}
        className={[armed && armCls, className].filter(Boolean).join(' ')}
        aria-label={armed ? `${label}: press again to confirm` : label}
        aria-describedby={tip && keyTip.open ? tipId : undefined}
        onClick={press}
        onKeyDown={onKeyDown}
        onBlur={cancel}
        disabled={disabled}
      >
        {armed ? armedLegend(children, label, armedChildren) : (children ?? label)}
      </Key>
      <span className="fp-sr" aria-live="polite">{armed ? `${label} armed, press again to confirm` : ''}</span>
      {tip && keyTip.open && <KeyTipBox id={tipId} text={tip} wrapRef={wrapRef} />}
    </span>
  );
}

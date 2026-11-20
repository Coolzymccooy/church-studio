import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react';
import { tipPosition } from '../../lib/tipPosition.js';

/**
 * The ⓘ key that carries every explanation, so the faceplate itself shows only labels and state.
 * Hover or focus shows the tip; a click pins it; Escape (wherever focus is) or a click elsewhere closes it.
 */
export default function InfoTip({ label, children }) {
  const id = useId();
  const wrap = useRef(null);
  const [hover, setHover] = useState(false);
  const [pinned, setPinned] = useState(false);
  const open = hover || pinned;
  const box = useRef(null);

  // Fixed placement measured from the key and the box itself, written straight to the box before
  // paint, so a panel's scroll area or edge cannot clip the tip.
  useLayoutEffect(() => {
    const el = box.current;
    if (!open || !el || !wrap.current) return;
    const { top, left } = tipPosition(wrap.current.getBoundingClientRect(), window.innerWidth, el.offsetWidth, el.offsetHeight, window.innerHeight);
    Object.assign(el.style, { position: 'fixed', top: `${top}px`, left: `${left}px`, transform: 'none' });
  }, [open]);

  useEffect(() => {
    if (!pinned) return undefined;
    const onDown = (e) => {
      if (wrap.current && !wrap.current.contains(e.target)) setPinned(false);
    };
    // Escape must still close a pinned tip after focus has moved to another control.
    const onEscape = (e) => {
      if (e.key === 'Escape') setPinned(false);
    };
    document.addEventListener('pointerdown', onDown);
    document.addEventListener('keydown', onEscape);
    return () => {
      document.removeEventListener('pointerdown', onDown);
      document.removeEventListener('keydown', onEscape);
    };
  }, [pinned]);

  const onKeyDown = (e) => {
    if (e.key === 'Escape' && open) {
      e.stopPropagation();
      setPinned(false);
      setHover(false);
    }
  };

  return (
    <span
      className="fp-tipwrap"
      ref={wrap}
      onPointerEnter={() => setHover(true)}
      onPointerLeave={() => setHover(false)}
    >
      <button
        type="button"
        className="fp-tip"
        aria-label={`About ${label}`}
        aria-expanded={open}
        aria-describedby={open ? id : undefined}
        onClick={() => setPinned((p) => !p)}
        onFocus={() => setHover(true)}
        onBlur={() => setHover(false)}
        onKeyDown={onKeyDown}
      >
        i
      </button>
      {open && (
        <span role="tooltip" id={id} className="fp-tipbox" ref={box}>
          {children}
        </span>
      )}
    </span>
  );
}

import { useEffect, useId, useRef, useState } from 'react';

/**
 * The ⓘ key that carries every explanation, so the faceplate itself shows only labels and state.
 * Hover or focus shows the tip; a click pins it; Escape or a click elsewhere closes it.
 */
export default function InfoTip({ label, children }) {
  const id = useId();
  const wrap = useRef(null);
  const [hover, setHover] = useState(false);
  const [pinned, setPinned] = useState(false);
  const open = hover || pinned;

  useEffect(() => {
    if (!pinned) return undefined;
    const onDown = (e) => {
      if (wrap.current && !wrap.current.contains(e.target)) setPinned(false);
    };
    document.addEventListener('pointerdown', onDown);
    return () => document.removeEventListener('pointerdown', onDown);
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
      {open && <span role="tooltip" id={id} className="fp-tipbox">{children}</span>}
    </span>
  );
}

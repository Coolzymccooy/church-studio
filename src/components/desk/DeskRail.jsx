import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';

import { popMaxHeight, popPosition } from '../../lib/deskRail.js';
import Key from '../faceplate/Key.jsx';

/** Vertical padding of .fp-pop-bd (10px top and bottom). */
const POP_BODY_PAD = 20;

const LED = { g: 'fp-led-g', a: 'fp-led-a', r: 'fp-led-r', o: 'fp-led-o', off: '' };

/** One faceplate icon key: the icon, a short legend, a status LED and an optional count. */
function RailKey({ item, open, onPress, keyRef }) {
  const Icon = item.icon;
  return (
    <button
      ref={keyRef}
      type="button"
      className={`fp-railkey${open ? ' fp-railkey-open' : ''}`}
      aria-expanded={open}
      aria-controls={`rail-pop-${item.id}`}
      aria-label={item.aria ?? item.title}
      onClick={onPress}
    >
      <span className={['fp-led', 'fp-railkey-led', LED[item.led ?? 'off']].filter(Boolean).join(' ')} aria-hidden="true" />
      {item.badge > 0 && (
        <span className={`fp-railkey-badge${item.led === 'r' ? '' : ' fp-railkey-badge-a'}`} aria-hidden="true">{item.badge > 9 ? '9+' : item.badge}</span>
      )}
      <Icon size={20} strokeWidth={1.75} aria-hidden="true" />
      <span className="fp-railkey-l">{item.label}</span>
    </button>
  );
}

/** The open popover, placed beside its key with fixed positioning measured before paint. */
function RailPop({ item, side, getAnchor, onClose }) {
  const ref = useRef(null);
  // Placement is measured from the key and the popover itself. Content can grow after opening
  // (a refreshed NDI source list), so a ResizeObserver places it again, and the height is capped
  // from the actual top so the bottom never leaves the window; the body scrolls inside instead.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return undefined;
    const place = () => {
      const anchor = getAnchor();
      if (!anchor) return;
      const viewport = { w: window.innerWidth, h: window.innerHeight };
      // The natural height (header plus the body's full content), not the capped one, so a tall
      // popover moves up to use the room it needs before its body has to scroll.
      const head = el.firstElementChild;
      const inner = el.querySelector('.fp-pop-in');
      const natural = (head?.offsetHeight ?? 0) + (inner?.offsetHeight ?? 0) + POP_BODY_PAD + 2;
      const pos = popPosition(anchor.getBoundingClientRect(), { w: el.offsetWidth, h: natural }, viewport, side);
      const style = { top: `${pos.top}px`, left: `${pos.left}px`, maxHeight: `${popMaxHeight(pos.top, viewport.h)}px` };
      for (const [k, v] of Object.entries(style)) if (el.style[k] !== v) el.style[k] = v;
    };
    place();
    const ro = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(place);
    // Watch the content, not the capped popover: a capped popover does not change size.
    const inner = el.querySelector('.fp-pop-in');
    ro?.observe(inner ?? el);
    window.addEventListener('resize', place);
    return () => {
      ro?.disconnect();
      window.removeEventListener('resize', place);
    };
  }, [getAnchor, side]);
  return (
    <section ref={ref} id={`rail-pop-${item.id}`} className="fp-pop" role="dialog" aria-modal="false" aria-label={item.title}>
      <div className="fp-pop-hd">
        <span>{item.title}</span>
        <span className="fp-keys" style={{ alignItems: 'center' }}>
          {item.headerExtra}
          <Key onClick={onClose} aria-label={`close ${item.title}`}>close</Key>
        </span>
      </div>
      <div className="fp-pop-bd"><div className="fp-pop-in">{item.render()}</div></div>
    </section>
  );
}

/**
 * A slim rail of faceplate icon keys. Each key shows its module's state on an LED; pressing it
 * opens that module in a popover beside the rail (one at a time). Escape, the key again, or a
 * press outside the rail and popover closes it.
 */
export default function DeskRail({ side, label, items, top = null }) {
  const [openId, setOpenId] = useState(null);
  const keys = useRef({});
  const railRef = useRef(null);
  const open = items.find((i) => i.id === openId) ?? null;
  const getAnchor = useCallback(() => (openId ? keys.current[openId] : null), [openId]);

  useEffect(() => {
    if (!open) return undefined;
    const onKey = (e) => {
      if (e.key !== 'Escape' || e.defaultPrevented) return;
      setOpenId(null);
      keys.current[open.id]?.focus();
    };
    const onDown = (e) => {
      if (railRef.current?.contains(e.target)) return;
      if (e.target.closest?.('.fp-pop, .fp-tipbox, .fp-scrim, .fp-drawer, .fp-dialog, .fp-setup')) return;
      setOpenId(null);
    };
    document.addEventListener('keydown', onKey);
    document.addEventListener('pointerdown', onDown);
    return () => {
      document.removeEventListener('keydown', onKey);
      document.removeEventListener('pointerdown', onDown);
    };
  }, [open]);

  return (
    <nav ref={railRef} className={`fp-rail fp-rail-${side}`} aria-label={label}>
      {top}
      {items.map((item) => (
        <RailKey
          key={item.id}
          item={item}
          open={openId === item.id}
          keyRef={(el) => { keys.current[item.id] = el; }}
          onPress={() => setOpenId((cur) => (cur === item.id ? null : item.id))}
        />
      ))}
      {open && <RailPop item={open} side={side} getAnchor={getAnchor} onClose={() => setOpenId(null)} />}
    </nav>
  );
}

import { useEffect, useRef } from 'react';

const COLOURS = new Set(['amber', 'clay', 'teal', 'olive', 'sky', 'rose', 'stone', 'bus']);

/**
 * A long-throw fader. `position` runs 0 (bottom) to 1 (top); the caller maps it to dB.
 * A hidden native range input carries keyboard, pointer and screen-reader behaviour; it is
 * rotated rather than using vertical writing mode so older desktop webviews behave the same.
 */
export default function Fader({
  position, onChange, onTouch, label, valueText, colour = 'stone', height = 196, marks = [], disabled = false,
}) {
  const endTouch = useRef(null);
  useEffect(() => () => endTouch.current?.(), []);

  const startTouch = () => {
    if (!onTouch || endTouch.current) return;
    onTouch(true);
    const refresh = window.setInterval(() => onTouch(true), 500);
    const end = () => {
      window.clearInterval(refresh);
      endTouch.current = null;
      window.removeEventListener('pointerup', end);
      window.removeEventListener('pointercancel', end);
      window.removeEventListener('blur', end);
      onTouch(false);
    };
    endTouch.current = end;
    window.addEventListener('pointerup', end);
    window.addEventListener('pointercancel', end);
    window.addEventListener('blur', end);
  };

  const travel = height - 18;
  const capBottom = 9 + position * travel;
  const cc = COLOURS.has(colour) ? colour : 'stone';
  return (
    <div className={`fp-ft fp-c-${cc}`} style={{ height }}>
      <span className="fp-slot" aria-hidden="true" />
      {marks.map((m) => (
        <span
          key={m.at}
          className={m.zero ? 'fp-tk fp-z0' : 'fp-tk'}
          style={{ bottom: 9 + m.at * travel }}
          aria-hidden="true"
        />
      ))}
      <span className="fp-fcap" style={{ bottom: capBottom }} aria-hidden="true" />
      <input
        type="range"
        className="fp-fr"
        min={0}
        max={1}
        step={0.001}
        value={position}
        onPointerDown={startTouch}
        onChange={(e) => onChange(Number(e.target.value))}
        aria-label={label}
        aria-valuetext={valueText}
        aria-orientation="vertical"
        disabled={disabled}
        style={{ width: height }}
      />
    </div>
  );
}

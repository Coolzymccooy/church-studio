import { useState } from 'react';
import { RING_DASH_GAP, RING_SWEEP, knobKeyStep, knobResetValue, knobView } from '../../lib/faceplateControls.js';
import KnobReadout from './KnobReadout.jsx';
import useKnobInput from './useKnobInput.js';

const JOBS = new Set(['trim', 'hpf', 'dyn', 'eq', 'pan', 'send']);

// The cap, in a 40-unit box: a dark knurled skirt with a coloured face, lit from above.
const SKIRT_R = 14;
const FACE_R = 10.4;
const BEVEL_R = 9.7;
const BEVEL_ARC = 2 * Math.PI * BEVEL_R * (130 / 360);
const BEVEL_DASH = `${BEVEL_ARC} ${2 * Math.PI * BEVEL_R}`;
const KNURL_PATH = Array.from({ length: 24 }, (_, i) => {
  const a = ((i * 15 + 7.5) * Math.PI) / 180;
  const p = (r) => `${(20 + r * Math.sin(a)).toFixed(2)} ${(20 - r * Math.cos(a)).toFixed(2)}`;
  return `M${p(FACE_R + 1.4)}L${p(SKIRT_R - 0.9)}`;
}).join('');

/**
 * A rotary knob. Drag up/down or left/right (Shift or Alt for fine), scroll or two-finger swipe
 * over it, or focus it and use the arrow keys (Shift for bigger steps, Page Up/Down, Home/End).
 * Double-click the knob to return to `defaultValue`; double-click the value (or press Enter) to
 * type one. `detent` is a value the knob rests on (0 dB, centre); `taper` maps value to turn.
 */
export default function Knob({
  value, onChange, min, max, step, bipolar = false, defaultValue, detent, taper, job = 'send', size = 'l',
  label, valueText, off = false, disabled = false,
}) {
  const range = { min, max, step, bipolar, detent: detent ?? (bipolar ? (min + max) / 2 : undefined), taper };
  const [editing, setEditing] = useState(false);
  const set = (next) => {
    if (!disabled && next !== null && next !== value) onChange(next);
  };
  // The hook tracks what it last sent, so it skips the comparison with a possibly stale `value`.
  const send = (next) => {
    if (!disabled) onChange(next);
  };
  const { elRef, live, handlers } = useKnobInput({ value, range, disabled, onSet: send });
  const view = knobView(value, range);
  const text = valueText ?? String(value);
  const cap = JOBS.has(job) ? job : 'send';

  const onKeyDown = (e) => {
    if (e.key === 'Enter' && !disabled) {
      e.preventDefault();
      setEditing(true);
      return;
    }
    const next = knobKeyStep(value, range, e.key, e.shiftKey);
    if (next === null) return;
    e.preventDefault();
    set(next);
  };
  const closeEditor = (next) => {
    setEditing(false);
    if (next !== undefined) set(next);
    elRef.current?.focus({ preventScroll: true });
  };

  return (
    <div className={['fp-kc', `fp-kc-${size}`, off && 'fp-off'].filter(Boolean).join(' ')}>
      <div
        ref={elRef}
        role="slider"
        tabIndex={disabled ? -1 : 0}
        aria-label={label}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={value}
        aria-valuetext={text}
        aria-disabled={disabled || undefined}
        className={`fp-knob fp-cap-${cap}${live ? ' fp-turning' : ''}`}
        {...handlers}
        onKeyDown={onKeyDown}
        onDoubleClick={() => set(knobResetValue(range, defaultValue))}
      >
        <svg className="fp-ring" viewBox="0 0 40 40" aria-hidden="true">
          <circle className="fp-trk" cx="20" cy="20" r="17" strokeDasharray={`${RING_SWEEP} ${RING_DASH_GAP}`} />
          <circle
            className="fp-val"
            cx="20"
            cy="20"
            r="17"
            strokeDasharray={`${view.arcLength} ${RING_DASH_GAP}`}
            style={{ transform: `rotate(${view.arcStart}deg)` }}
          />
          <circle className="fp-shadow" cx="20" cy="21.3" r={SKIRT_R} />
          <g transform={`rotate(${view.pointerDeg} 20 20)`}>
            <circle className="fp-skirt" cx="20" cy="20" r={SKIRT_R} />
            <path className="fp-knurl" d={KNURL_PATH} />
            <circle className="fp-face" cx="20" cy="20" r={FACE_R} />
            <line className="fp-ptr-skirt" x1="20" y1={20 - SKIRT_R + 0.9} x2="20" y2={20 - FACE_R - 0.3} />
            <line className="fp-ptr" x1="20" y1={20 - FACE_R + 1.6} x2="20" y2="16" />
          </g>
          <circle className="fp-bevel fp-bevel-hi" cx="20" cy="20" r={BEVEL_R} strokeDasharray={BEVEL_DASH} transform="rotate(-155 20 20)" />
          <circle className="fp-bevel fp-bevel-lo" cx="20" cy="20" r={BEVEL_R} strokeDasharray={BEVEL_DASH} transform="rotate(25 20 20)" />
        </svg>
      </div>
      <KnobReadout
        text={text}
        label={label}
        range={range}
        live={live}
        editing={editing}
        disabled={disabled}
        onOpen={() => setEditing(true)}
        onClose={closeEditor}
      />
      <span className="fp-kl" aria-hidden="true">{label}</span>
    </div>
  );
}

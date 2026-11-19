import { useRef } from 'react';
import { RING_DASH_GAP, RING_SWEEP, knobDragValue, knobKeyStep, knobView } from '../../lib/faceplateControls.js';

const JOBS = new Set(['trim', 'hpf', 'dyn', 'eq', 'pan', 'send']);

/**
 * A rotary knob. Drag up or down, use the arrow keys (shift for bigger steps, Home and End for the
 * stops), or double-click to return to `defaultValue`. The cap colour says what the knob does.
 */
export default function Knob({
  value, onChange, min, max, step, bipolar = false, defaultValue, job = 'send', size = 'l',
  label, valueText, off = false, disabled = false,
}) {
  const range = { min, max, step, bipolar };
  const drag = useRef(null);
  const view = knobView(value, range);
  const text = valueText ?? String(value);
  const cap = JOBS.has(job) ? job : 'send';

  const set = (next) => {
    if (!disabled && next !== null && next !== value) onChange(next);
  };
  const onPointerDown = (e) => {
    if (disabled || e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { y: e.clientY, value };
  };
  const onPointerMove = (e) => {
    if (drag.current) set(knobDragValue(drag.current.value, e.clientY - drag.current.y, range));
  };
  const endDrag = () => { drag.current = null; };
  const onKeyDown = (e) => {
    const next = knobKeyStep(value, range, e.key, e.shiftKey);
    if (next === null) return;
    e.preventDefault();
    set(next);
  };
  const onDoubleClick = () => {
    if (defaultValue !== undefined) set(defaultValue);
  };

  return (
    <div className={['fp-kc', `fp-kc-${size}`, off && 'fp-off'].filter(Boolean).join(' ')}>
      <div
        role="slider"
        tabIndex={disabled ? -1 : 0}
        aria-label={label}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={value}
        aria-valuetext={text}
        aria-disabled={disabled || undefined}
        className={`fp-knob fp-cap-${cap}`}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onKeyDown={onKeyDown}
        onDoubleClick={onDoubleClick}
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
        </svg>
        <span className="fp-cap" style={{ transform: `rotate(${view.pointerDeg}deg)` }}>
          <span className="fp-ptr" />
        </span>
      </div>
      <span className="fp-kv" aria-hidden="true">{text}</span>
      <span className="fp-kl" aria-hidden="true">{label}</span>
    </div>
  );
}

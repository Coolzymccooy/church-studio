import Key from './Key.jsx';
import useTwoPress from './useTwoPress.js';

/**
 * A key that needs two presses: engine stop, scene load or delete, stream, record stop, reset.
 * The first press arms it (amber edge, or red for `tone="red"`); it disarms after 3 s, on Escape
 * or when focus leaves.
 */
export default function TwoPressKey({ onConfirm, tone = 'amber', label, children, lit = null, disabled, className = '' }) {
  const { armed, press, cancel } = useTwoPress(onConfirm);
  const armCls = tone === 'red' ? 'fp-arm-r' : 'fp-arm';
  const onKeyDown = (e) => {
    if (e.key === 'Escape' && armed) {
      e.stopPropagation();
      cancel();
    }
  };
  return (
    <>
      <Key
        lit={armed ? null : lit}
        className={[armed && armCls, className].filter(Boolean).join(' ')}
        aria-label={armed ? `${label}: press again to confirm` : label}
        onClick={press}
        onKeyDown={onKeyDown}
        onBlur={cancel}
        disabled={disabled}
      >
        {children ?? label}
      </Key>
      <span className="fp-sr" aria-live="polite">{armed ? `${label} armed, press again to confirm` : ''}</span>
    </>
  );
}

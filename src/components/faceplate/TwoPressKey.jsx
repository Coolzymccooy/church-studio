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

export default function TwoPressKey({
  onConfirm, tone = 'amber', label, children, armedChildren, lit = null, disabled, className = '',
}) {
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
        {armed ? armedLegend(children, label, armedChildren) : (children ?? label)}
      </Key>
      <span className="fp-sr" aria-live="polite">{armed ? `${label} armed, press again to confirm` : ''}</span>
    </>
  );
}

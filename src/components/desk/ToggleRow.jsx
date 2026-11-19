import InfoTip from '../faceplate/InfoTip.jsx';
import Key from '../faceplate/Key.jsx';

/**
 * One switchable stage: its label, an optional readout, the explanation in an ⓘ tip, and an on/off key.
 * `tone` is the lit colour by meaning: orange for processing, green for a healthy safety stage.
 */
export default function ToggleRow({ label, tip, on, onToggle, readout = null, tone = 'orange', disabled }) {
  return (
    <div className="fp-row">
      <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
        {label}
        {tip && <InfoTip label={label}>{tip}</InfoTip>}
      </span>
      <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }}>
        {readout !== null && <span className="fp-num fp-sub">{readout}</span>}
        <Key lit={on ? tone : null} pressed={on} onClick={onToggle} disabled={disabled} aria-label={label} style={{ minWidth: 36 }}>
          {on ? 'on' : 'off'}
        </Key>
      </span>
    </div>
  );
}

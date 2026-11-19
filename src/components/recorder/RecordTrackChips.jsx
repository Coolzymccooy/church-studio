import { armedIndices } from '../../lib/recorder.js';
import Key from '../faceplate/Key.jsx';

/**
 * Armed-track keys: one per mixer strip (press to arm or skip it), then the Stream mix (always
 * recorded) and the Main mix (when included).
 */
export default function RecordTrackChips({ strips, armedStrips, includeMain, disabled, onToggle }) {
  const indices = strips.map((strip) => strip.index);
  const armed = new Set(armedIndices(armedStrips, indices));
  return (
    <div className="fp-keys" role="group" aria-label="Tracks to record" style={{ alignItems: 'center' }}>
      {strips.map((strip) => {
        const on = armed.has(strip.index);
        return (
          <Key
            key={strip.index}
            lit={on ? 'red' : null}
            pressed={on}
            disabled={disabled}
            onClick={() => onToggle(strip.index)}
            title={on ? `Recording ${strip.name} (raw)` : `Not recording ${strip.name}`}
          >
            {String(strip.index + 1).padStart(2, '0')} {strip.name}
          </Key>
        );
      })}
      <span className="fp-sub">+ stream mix{includeMain ? ' + main mix' : ''}</span>
    </div>
  );
}

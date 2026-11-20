import { useRef, useState } from 'react';
import { parseKnobText } from '../../lib/knobTravel.js';

/**
 * The value under a knob. It lights while the knob is turned. Double-click it to type a value:
 * Enter or leaving the field sets it, Escape cancels, and unreadable text changes nothing.
 */
export default function KnobReadout({ text, label, range, live, editing, disabled, onOpen, onClose }) {
  if (editing) return <KnobEntry text={text} label={label} range={range} onClose={onClose} />;
  return (
    <span
      className={`fp-kv${live ? ' fp-kv-live' : ''}`}
      aria-hidden="true"
      onDoubleClick={disabled ? undefined : onOpen}
    >
      {text}
    </span>
  );
}

function KnobEntry({ text, label, range, onClose }) {
  const [draft, setDraft] = useState(text);
  const done = useRef(false);
  const finish = (commit) => {
    if (done.current) return;
    done.current = true;
    const next = commit ? parseKnobText(draft, range) : null;
    onClose(next === null ? undefined : next);
  };
  return (
    <input
      className="fp-kv fp-kv-edit"
      value={draft}
      size={5}
      autoFocus
      spellCheck={false}
      aria-label={`${label} value`}
      onFocus={(e) => e.currentTarget.select()}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => finish(true)}
      onKeyDown={(e) => {
        if (e.key === 'Enter') { e.preventDefault(); finish(true); }
        if (e.key === 'Escape') { e.preventDefault(); finish(false); }
      }}
    />
  );
}

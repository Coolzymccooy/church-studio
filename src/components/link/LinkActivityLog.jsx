import { formatTimeAgo } from '../../lib/linkControl.js';
import Key from '../faceplate/Key.jsx';

/** Colour by meaning: a failed load is a fault, a skipped one a caution; the rest stay plain. */
function actionTone(action) {
  if (action.startsWith('failed')) return 'fp-fault';
  if (action.startsWith('skipped')) return 'fp-caution';
  return '';
}

const ELLIPSIS = { overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' };

/** Live activity log, newest first, with Undo on automatic scene loads. */
export default function LinkActivityLog({ entries, now, onUndo }) {
  if (!entries.length) {
    return <p className="fp-sub" style={{ margin: 0 }}>no activity yet</p>;
  }
  return (
    <ul
      className="fp-glass"
      style={{ maxHeight: 192, overflowY: 'auto', margin: 0, padding: '4px 8px', listStyle: 'none', fontSize: 'var(--fp-text-xs)' }}
      aria-label="Lumina Link activity"
    >
      {entries.map((entry) => (
        <li key={entry.id} style={{ display: 'flex', alignItems: 'center', gap: 8, minHeight: 24, padding: '2px 0' }}>
          <span className="fp-glass-dim" style={{ width: 56, flex: 'none' }}>{formatTimeAgo(entry.ts, now)}</span>
          <span className="fp-glass-dim" style={{ width: 160, flex: 'none', ...ELLIPSIS }} title={entry.event}>
            {entry.event.replace(/^lumina\./, '')}
            {entry.contentKind ? ` · ${entry.contentKind}` : ''}
          </span>
          <span className={actionTone(entry.action)} style={{ flex: 1, minWidth: 0, ...ELLIPSIS }} title={entry.action}>
            {entry.action}
            {entry.previousScene && entry.loadedScene ? ` (was ${entry.previousScene})` : ''}
          </span>
          {entry.undoable && (
            <Key
              onClick={() => onUndo(entry.id)}
              title={`Reload ${entry.previousScene}`}
              aria-label={`undo: reload ${entry.previousScene}`}
            >
              undo
            </Key>
          )}
        </li>
      ))}
    </ul>
  );
}

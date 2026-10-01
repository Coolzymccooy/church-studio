import { Undo2 } from 'lucide-react';

import { formatTimeAgo } from '../../lib/linkControl.js';

function actionTone(action) {
  if (action.startsWith('loaded') || action.startsWith('undo')) return 'text-emerald-400';
  if (action.startsWith('skipped') || action.startsWith('failed')) return 'text-amber-400';
  return 'text-slate-400';
}

/** Live activity log, newest first, with Undo on automatic scene loads. */
export default function LinkActivityLog({ entries, now, onUndo }) {
  if (!entries.length) {
    return (
      <p className="text-[10px] text-slate-500 py-2">
        No activity yet. Scene changes triggered by Lumina appear here.
      </p>
    );
  }
  return (
    <ul className="max-h-48 overflow-y-auto divide-y divide-slate-800">
      {entries.map((entry) => (
        <li key={entry.id} className="flex items-center gap-2 py-1 text-[10px]">
          <span className="w-14 shrink-0 text-slate-600 font-mono">{formatTimeAgo(entry.ts, now)}</span>
          <span className="w-40 shrink-0 truncate text-slate-500" title={entry.event}>
            {entry.event.replace(/^lumina\./, '')}
            {entry.contentKind ? ` · ${entry.contentKind}` : ''}
          </span>
          <span className={`flex-1 truncate ${actionTone(entry.action)}`} title={entry.action}>
            {entry.action}
            {entry.previousScene && entry.loadedScene ? ` (was ${entry.previousScene})` : ''}
          </span>
          {entry.undoable && (
            <button
              type="button"
              onClick={() => onUndo(entry.id)}
              className="flex items-center gap-1 px-1.5 py-0.5 rounded border border-slate-700 text-slate-300 hover:text-white"
              title={`Reload ${entry.previousScene}`}
            >
              <Undo2 size={10} /> Undo
            </button>
          )}
        </li>
      ))}
    </ul>
  );
}

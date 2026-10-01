import React from 'react';
import { AlertTriangle } from 'lucide-react';

/** Small confirm dialog layered over the OBS panel. `request` null = hidden. */
export default function ObsConfirmDialog({ request, onConfirm, onCancel }) {
  if (!request) return null;
  return (
    <div
      className="absolute inset-0 z-10 flex items-center justify-center bg-black/60 rounded-2xl p-4"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="obs-confirm-title"
    >
      <div className="w-full max-w-sm rounded-xl border border-slate-700 bg-slate-900 p-5 space-y-3 shadow-2xl">
        <div className="flex items-center gap-2">
          <AlertTriangle size={16} className="text-amber-400 shrink-0" />
          <h3 id="obs-confirm-title" className="text-sm font-bold text-white">{request.title}</h3>
        </div>
        <p className="text-xs text-slate-400 leading-relaxed">{request.message}</p>
        <div className="flex justify-end gap-2 pt-1">
          <button
            type="button"
            onClick={onCancel}
            className="px-3 py-1.5 rounded-lg text-xs text-slate-300 bg-slate-800 hover:bg-slate-700"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={onConfirm}
            autoFocus
            className={`px-3 py-1.5 rounded-lg text-xs font-bold text-white ${
              request.danger ? 'bg-red-600 hover:bg-red-500' : 'bg-emerald-600 hover:bg-emerald-500'
            }`}
          >
            {request.confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

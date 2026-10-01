import React from 'react';
import { Circle, Radio } from 'lucide-react';

function OutputButton({ active, icon, idleLabel, activeLabel, timecode, disabled, onClick }) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      aria-pressed={active}
      className={`flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded-lg text-xs font-bold border transition-colors disabled:opacity-40 ${
        active
          ? 'bg-red-600/20 border-red-500/60 text-red-300 hover:bg-red-600/30'
          : 'bg-slate-800 border-slate-700 text-slate-200 hover:bg-slate-700'
      }`}
    >
      {icon}
      {active ? activeLabel : idleLabel}
      {active && timecode && <span className="font-mono text-[10px] opacity-80">{timecode}</span>}
    </button>
  );
}

/** OBS scene picker plus Stream / Record buttons (each behind a confirm). */
export default function ObsOutputSection({ status, busy, onSetScene, onRequestAction }) {
  const connected = Boolean(status?.connected);
  const scenes = status?.scenes || [];
  const current = status?.currentScene || '';

  return (
    <section className="space-y-3">
      <label className="block text-[10px] uppercase font-bold text-slate-500">
        OBS scene (program)
        <select
          className="w-full bg-slate-950 border border-slate-700 rounded-lg px-2.5 py-1.5 text-xs text-slate-200"
          value={current}
          disabled={!connected || busy || scenes.length === 0}
          onChange={(e) => e.target.value && onSetScene(e.target.value)}
        >
          {!current && <option value="">—</option>}
          {scenes.map((scene) => (
            <option key={scene} value={scene}>{scene}</option>
          ))}
        </select>
      </label>

      <div className="flex gap-2">
        <OutputButton
          active={Boolean(status?.streaming)}
          icon={<Radio size={13} />}
          idleLabel="Start stream"
          activeLabel="LIVE · Stop"
          timecode={status?.streamTimecode}
          disabled={!connected || busy}
          onClick={() => onRequestAction(status?.streaming ? 'stopStream' : 'startStream')}
        />
        <OutputButton
          active={Boolean(status?.recording)}
          icon={<Circle size={12} />}
          idleLabel="Start recording"
          activeLabel="REC · Stop"
          timecode={status?.recordTimecode}
          disabled={!connected || busy}
          onClick={() => onRequestAction(status?.recording ? 'stopRecord' : 'startRecord')}
        />
      </div>
    </section>
  );
}

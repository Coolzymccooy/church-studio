import React, { useMemo, useState } from 'react';
import { Circle, Flag, FolderOpen, Square } from 'lucide-react';

import { isTauri } from '../../lib/platform';
import {
  formatElapsed,
  getDesktopRecorderController,
  recorderHealth,
  toggleArmedStrip,
} from '../../lib/recorder.js';
import { useRecorder } from './useRecorder.js';
import RecordTrackChips from './RecordTrackChips.jsx';

const HEALTH_COLORS = { ok: 'text-slate-500', warn: 'text-amber-400', error: 'text-[#FF5252]' };

async function pickFolder(defaultPath) {
  const { open } = await import('@tauri-apps/plugin-dialog');
  const chosen = await open({ directory: true, multiple: false, defaultPath: defaultPath || undefined });
  return typeof chosen === 'string' ? chosen : null;
}

/**
 * Record section of the MIXER tab: multitrack recording of every armed
 * strip (raw) plus the Stream mix. Desktop only; the browser build
 * explains that.
 */
export default function RecordPanel({ strips, engineRunning }) {
  if (!isTauri) {
    return (
      <div className="px-3 py-1.5 border-b border-slate-800 text-[9px] text-slate-500" style={{ background: '#0A1124' }}>
        <span className="font-bold uppercase tracking-wider text-slate-400 mr-2">Record</span>
        Multitrack service recording (one WAV per channel) runs in the TIWATON AI Studio desktop app.
      </div>
    );
  }
  return <RecordSection strips={strips} engineRunning={engineRunning} />;
}

function RecordSection({ strips, engineRunning }) {
  const controller = useMemo(() => getDesktopRecorderController(), []);
  const rec = useRecorder(controller);
  const { status, config, busy } = rec;
  const [title, setTitle] = useState('');
  const [markerLabel, setMarkerLabel] = useState('');
  const [confirmStop, setConfirmStop] = useState(false);

  const recording = status.recording;
  // The mixer's `running` can lag an engine start; the recorder polls it.
  const engineOn = status.engineRunning || engineRunning;
  const canStart = engineOn && !busy && !recording;
  const health = recorderHealth(status);
  const folder = config ? (config.folder || config.defaultFolder || '') : '';
  const summary = !recording ? status.lastSummary : null;

  const toggleStrip = (index) => {
    if (!config) return;
    const indices = strips.map((strip) => strip.index);
    rec.saveConfig({ ...config, armedStrips: toggleArmedStrip(config.armedStrips, index, indices) });
  };

  const chooseFolder = async () => {
    const chosen = await pickFolder(folder).catch(() => null);
    if (chosen && config) rec.saveConfig({ ...config, folder: chosen });
  };

  const handleStop = async () => {
    setConfirmStop(false);
    await rec.stop();
  };

  const handleMarker = async () => {
    await rec.addMarker(markerLabel);
    setMarkerLabel('');
  };

  return (
    <div className="shrink-0 border-b border-slate-800 px-3 py-2 space-y-1.5 text-[9px]" style={{ background: '#0A1124' }}>
      <div className="flex items-center gap-2 flex-wrap">
        <span className="font-bold uppercase tracking-wider text-slate-400">Record</span>
        {!recording && (
          <button
            type="button"
            onClick={() => rec.start(title)}
            disabled={!canStart}
            title={engineOn ? 'Start a multitrack recording' : 'Start the engine to record'}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-full font-bold text-[10px] text-white bg-red-600 hover:bg-red-500 disabled:opacity-40"
          >
            <Circle size={10} fill="currentColor" /> REC
          </button>
        )}
        {recording && !confirmStop && (
          <button
            type="button"
            onClick={() => setConfirmStop(true)}
            disabled={busy}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-full font-bold text-[10px] text-white bg-slate-700 hover:bg-slate-600"
          >
            <Square size={10} fill="currentColor" /> STOP
          </button>
        )}
        {recording && confirmStop && (
          <span className="flex items-center gap-1" role="alertdialog" aria-label="Confirm stop recording">
            <span className="text-amber-400 font-semibold">Stop recording?</span>
            <button type="button" onClick={handleStop} autoFocus className="px-2 py-1 rounded bg-red-600 text-white font-bold">Stop</button>
            <button type="button" onClick={() => setConfirmStop(false)} className="px-2 py-1 rounded bg-slate-800 text-slate-300">Keep recording</button>
          </span>
        )}
        <span
          className={`font-mono text-[12px] font-bold ${recording ? 'text-[#FF5252]' : 'text-slate-600'}`}
          aria-label="Elapsed recording time"
        >
          {formatElapsed(recording ? status.elapsedSeconds : 0)}
        </span>
        {!recording && (
          <input
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder="Title (optional)"
            maxLength={60}
            aria-label="Recording title"
            className="w-36 px-2 py-1 rounded border border-slate-700 bg-[#050A1C] text-white outline-none focus:border-[var(--accent)]"
          />
        )}
        {recording && (
          <span className="flex items-center gap-1">
            <input
              value={markerLabel}
              onChange={(event) => setMarkerLabel(event.target.value)}
              onKeyDown={(event) => { if (event.key === 'Enter') handleMarker(); }}
              placeholder="Marker label"
              maxLength={80}
              aria-label="Marker label"
              className="w-28 px-2 py-1 rounded border border-slate-700 bg-[#050A1C] text-white outline-none focus:border-[var(--accent)]"
            />
            <button
              type="button"
              onClick={handleMarker}
              disabled={busy}
              className="flex items-center gap-1 px-2 py-1 rounded border border-slate-700 text-slate-300 hover:text-white"
            >
              <Flag size={10} /> Add marker
            </button>
            <span className="text-slate-500">{status.markers} marker{status.markers === 1 ? '' : 's'}</span>
          </span>
        )}
        {!engineOn && !recording && <span className="text-slate-500">Start the engine to record.</span>}
      </div>

      <RecordTrackChips
        strips={strips}
        armedStrips={config?.armedStrips ?? null}
        includeMain={Boolean(config?.includeMain)}
        disabled={!config || busy || recording}
        onToggle={toggleStrip}
      />

      <div className="flex items-center gap-3 flex-wrap text-slate-400">
        <label className="flex items-center gap-1 cursor-pointer">
          <input
            type="checkbox"
            className="accent-[var(--accent)]"
            checked={Boolean(config?.includeMain)}
            disabled={!config || busy || recording}
            onChange={(event) => rec.saveConfig({ ...config, includeMain: event.target.checked })}
          />
          Include Main mix
        </label>
        <span className="truncate max-w-[320px]" title={folder}>Folder: <span className="text-slate-300">{folder || '—'}</span></span>
        <button type="button" onClick={chooseFolder} disabled={!config || busy || recording} className="text-[var(--accent)] hover:underline disabled:opacity-40">
          Change…
        </button>
        {config?.folder && !recording && (
          <button type="button" onClick={() => rec.saveConfig({ ...config, folder: null })} className="text-slate-500 hover:underline">
            Use default
          </button>
        )}
        <span className={HEALTH_COLORS[health.level]}>{health.text}</span>
      </div>

      {summary && (
        <div className="flex items-center gap-2 flex-wrap text-slate-400">
          <span>
            Saved {summary.files?.length ?? 0} files · {formatElapsed(summary.durationSeconds)} · {summary.markers ?? 0} markers
            {summary.droppedFrames > 0 ? ` · ${summary.droppedFrames} dropped frames` : ''}
          </span>
          <button type="button" onClick={rec.openFolder} className="flex items-center gap-1 text-[var(--accent)] hover:underline">
            <FolderOpen size={10} /> Open folder
          </button>
        </div>
      )}

      {rec.error && (
        <div role="alert" className="flex items-center gap-2 text-[#FF5252]">
          {rec.error}
          <button type="button" onClick={rec.dismissError} aria-label="Dismiss recorder error" className="text-slate-500 hover:text-white">✕</button>
        </div>
      )}
    </div>
  );
}

import { useMemo, useState } from 'react';

import InfoTip from '../faceplate/InfoTip.jsx';
import Key from '../faceplate/Key.jsx';

import { isTauri } from '../../lib/platform';
import {
  formatElapsed,
  getDesktopRecorderController,
  isStopArmed,
  recorderHealth,
  stopArmKey,
  toggleArmedStrip,
} from '../../lib/recorder.js';
import { useRecorder } from './useRecorder.js';
import RecordTrackChips from './RecordTrackChips.jsx';

const HEALTH_CLASS = { ok: '', warn: 'fp-caution', error: 'fp-fault' };

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
      <section className="fp-recplate" aria-label="Record">
        <div className="fp-recrow">
          <span className="fp-label" style={{ margin: 0 }}>Record</span>
          <span className="fp-sub">desktop app only</span>
        </div>
      </section>
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
  // The "Stop recording?" prompt is armed for one recording (its
  // `stopArmKey`). If that recording ends from the centre-bar button, an
  // engine stop or a disk error, the next one must not open with the prompt
  // up and its Stop button focused.
  const [armedKey, setArmedKey] = useState(null);
  const confirmStop = isStopArmed(status, armedKey);

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
    setArmedKey(null);
    await rec.stop();
  };

  const handleMarker = async () => {
    // Enter and the button share this path; one marker per press, and the
    // typed label survives a failed add (`addMarker` returns null then).
    if (busy) return;
    const added = await rec.addMarker(markerLabel);
    if (added) setMarkerLabel('');
  };

  return (
    <section className="fp-recplate" aria-label="Record">
      <div className="fp-recrow">
        <span className="fp-label" style={{ margin: 0 }}>
          Record
          <InfoTip label="Record">One WAV per armed channel, raw, plus the Stream mix as it went to broadcast.</InfoTip>
        </span>
        {!recording && (
          <Key onClick={() => { setArmedKey(null); rec.start(title); }} disabled={!canStart}
            title={engineOn ? 'Start a multitrack recording' : 'Start the engine to record'}>
            rec
          </Key>
        )}
        {recording && !confirmStop && (
          <Key lit="red" onClick={() => setArmedKey(stopArmKey(status))} disabled={busy}>stop</Key>
        )}
        {recording && confirmStop && (
          <span className="fp-keys" role="alertdialog" aria-label="Confirm stop recording">
            <Key className="fp-arm-r" onClick={handleStop} autoFocus>stop now</Key>
            <Key onClick={() => setArmedKey(null)}>keep recording</Key>
          </span>
        )}
        <span className={`fp-glass fp-num ${recording ? 'fp-fault' : 'fp-glass-dim'}`} style={{ padding: '4px 8px' }}
          aria-label="Elapsed recording time">
          {formatElapsed(recording ? status.elapsedSeconds : 0)}
        </span>
        {!recording && (
          <input value={title} onChange={(event) => setTitle(event.target.value)} placeholder="Title" maxLength={60}
            aria-label="Recording title" className="fp-field" style={{ width: 160 }} />
        )}
        {recording && (
          <span className="fp-keys" style={{ alignItems: 'center' }}>
            <input value={markerLabel} onChange={(event) => setMarkerLabel(event.target.value)}
              onKeyDown={(event) => { if (event.key === 'Enter') handleMarker(); }}
              placeholder="Marker" maxLength={80} aria-label="Marker label" className="fp-field" style={{ width: 140 }} />
            <Key onClick={handleMarker} disabled={busy}>marker</Key>
            <span className="fp-sub fp-num">{status.markers}</span>
          </span>
        )}
        {!engineOn && !recording && <span className="fp-sub">engine off</span>}
      </div>

      <RecordTrackChips
        strips={strips}
        armedStrips={config?.armedStrips ?? null}
        includeMain={Boolean(config?.includeMain)}
        disabled={!config || busy || recording}
        onToggle={toggleStrip}
      />

      <div className="fp-recrow">
        <Key lit={config?.includeMain ? 'sel' : null} pressed={Boolean(config?.includeMain)}
          disabled={!config || busy || recording}
          onClick={() => rec.saveConfig({ ...config, includeMain: !config.includeMain })}>
          main mix
        </Key>
        <span className="fp-sub" style={{ maxWidth: 320, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }} title={folder}>
          {folder || 'no folder'}
        </span>
        <Key onClick={chooseFolder} disabled={!config || busy || recording}>folder</Key>
        {config?.folder && !recording && (
          <Key onClick={() => rec.saveConfig({ ...config, folder: null })}>default</Key>
        )}
        <span className={`fp-sub ${HEALTH_CLASS[health.level]}`}>{health.text}</span>
      </div>

      {summary && (
        <div className="fp-recrow">
          <span className="fp-sub">
            saved {summary.files?.length ?? 0} files · {formatElapsed(summary.durationSeconds)} · {summary.markers ?? 0} markers
            {summary.droppedFrames > 0 ? ` · ${summary.droppedFrames} dropped frames` : ''}
          </span>
          <Key onClick={rec.openFolder}>open folder</Key>
        </div>
      )}

      {rec.error && (
        <div role="alert" className="fp-recrow fp-fault">
          {rec.error}
          <Key onClick={rec.dismissError} aria-label="Dismiss recorder error">dismiss</Key>
        </div>
      )}
    </section>
  );
}

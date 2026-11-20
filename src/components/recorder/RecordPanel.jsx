import { useMemo, useState } from 'react';

import Glass from '../faceplate/Glass.jsx';
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
      <section className="fp-plate" aria-label="Record">
        <div className="fp-plate-hd"><span>Record</span><span className="fp-sub">desktop app only</span></div>
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

  const recKeyCls = ['fp-reckey', recording && !confirmStop && 'fp-reckey-on', recording && confirmStop && 'fp-reckey-arm'].filter(Boolean).join(' ');
  const recLabel = !recording ? 'start recording' : confirmStop ? 'press again to stop recording' : 'stop recording';
  const onRecKey = () => {
    if (!recording) {
      setArmedKey(null);
      rec.start(title);
    } else if (!confirmStop) {
      setArmedKey(stopArmKey(status));
    } else {
      handleStop();
    }
  };

  return (
    <section className="fp-plate" aria-label="Record">
      <div className="fp-plate-hd">
        <span>
          Record
          <InfoTip label="Record">One WAV per armed channel, raw, plus the Stream mix as it went to broadcast. Stop needs two presses.</InfoTip>
        </span>
        <span className={`fp-sub ${HEALTH_CLASS[health.level]}`}>{health.text}</span>
      </div>

      <div className="fp-recrow">
        <button type="button" className={recKeyCls} onClick={onRecKey} disabled={recording ? busy : !canStart}
          aria-label={recLabel} aria-pressed={recording} title={engineOn ? recLabel : 'Start the engine to record'}>
          <span className="fp-reckey-dot" aria-hidden="true" />
        </button>
        <Glass className="fp-timer" aria-label="Elapsed recording time">
          {formatElapsed(recording ? status.elapsedSeconds : 0)}
        </Glass>
        {recording && confirmStop && (
          <>
            <span className="fp-arm-ask" aria-hidden="true">stop?</span>
            <Key onClick={() => setArmedKey(null)}>keep</Key>
          </>
        )}
        {!recording && (
          <input value={title} onChange={(event) => setTitle(event.target.value)} placeholder="Title" maxLength={60}
            aria-label="Recording title" className="fp-field" style={{ flex: 1, minWidth: 0 }} />
        )}
      </div>

      {recording ? (
        <div className="fp-recrow">
          <input value={markerLabel} onChange={(event) => setMarkerLabel(event.target.value)}
            onKeyDown={(event) => { if (event.key === 'Enter') handleMarker(); }}
            placeholder="Marker" maxLength={80} aria-label="Marker label" className="fp-field" style={{ flex: 1, minWidth: 0 }} />
          <Key onClick={handleMarker} disabled={busy}>marker</Key>
          <span className="fp-num fp-sub" aria-label="Markers">{status.markers}</span>
        </div>
      ) : (
        <RecordTrackChips
          strips={strips}
          armedStrips={config?.armedStrips ?? null}
          includeMain={Boolean(config?.includeMain)}
          disabled={!config || busy}
          onToggle={toggleStrip}
        />
      )}

      <div className="fp-recrow">
        <Key lit={config?.includeMain ? 'sel' : null} pressed={Boolean(config?.includeMain)}
          disabled={!config || busy || recording}
          onClick={() => rec.saveConfig({ ...config, includeMain: !config.includeMain })}>
          + main mix
        </Key>
        <Key onClick={chooseFolder} disabled={!config || busy || recording} title={folder}>folder</Key>
        {config?.folder && !recording && <Key onClick={() => rec.saveConfig({ ...config, folder: null })}>default</Key>}
        {summary && <Key onClick={rec.openFolder}>open last</Key>}
      </div>

      {summary && (
        <div className="fp-sub">
          saved {summary.files?.length ?? 0} files · {formatElapsed(summary.durationSeconds)} · {summary.markers ?? 0} markers
          {summary.droppedFrames > 0 ? ` · ${summary.droppedFrames} dropped frames` : ''}
        </div>
      )}
      {!engineOn && !recording && <div className="fp-sub">engine off</div>}

      {rec.error && (
        <div role="alert" className="fp-recrow fp-fault">
          {rec.error}
          <Key onClick={rec.dismissError} aria-label="Dismiss recorder error">dismiss</Key>
        </div>
      )}
    </section>
  );
}

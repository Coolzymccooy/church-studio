import React, { useId } from 'react';

import Key from '../faceplate/Key.jsx';
import Lamp from '../faceplate/Lamp.jsx';

function OutputRow({ label, active, lit, onWord, idleLegend, activeLegend, timecode, disabled, onClick }) {
  return (
    <div className="fp-row">
      <span>{label}</span>
      <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }}>
        <Lamp tone={active ? (lit === 'red' ? 'r' : 'g') : 'off'}>{active ? onWord : 'off'}</Lamp>
        {active && timecode && <span className="fp-num fp-sub">{timecode}</span>}
        <Key lit={active ? lit : null} pressed={active} disabled={disabled} onClick={onClick}>
          {active ? activeLegend : idleLegend}
        </Key>
      </span>
    </div>
  );
}

/** OBS scene picker plus Stream / Record keys (each behind a confirm). */
export default function ObsOutputSection({ status, busy, onSetScene, onRequestAction }) {
  const id = useId();
  const connected = Boolean(status?.connected);
  const scenes = status?.scenes || [];
  const current = status?.currentScene || '';

  return (
    <section className="fp-mod" aria-label="OBS output">
      <div className="fp-mod-hd"><span>Output</span></div>
      <div className="fp-mod-bd">
        <div>
          <label className="fp-label" htmlFor={`${id}-scene`}>OBS scene (program)</label>
          <select
            id={`${id}-scene`}
            className="fp-field"
            value={current}
            disabled={!connected || busy || scenes.length === 0}
            onChange={(e) => e.target.value && onSetScene(e.target.value)}
          >
            {!current && <option value="">—</option>}
            {scenes.map((scene) => (
              <option key={scene} value={scene}>{scene}</option>
            ))}
          </select>
        </div>

        <OutputRow
          label="Stream"
          active={Boolean(status?.streaming)}
          lit="green"
          onWord="live"
          idleLegend="start stream"
          activeLegend="stop stream"
          timecode={status?.streamTimecode}
          disabled={!connected || busy}
          onClick={() => onRequestAction(status?.streaming ? 'stopStream' : 'startStream')}
        />
        <OutputRow
          label="Record"
          active={Boolean(status?.recording)}
          lit="red"
          onWord="rec"
          idleLegend="start recording"
          activeLegend="stop recording"
          timecode={status?.recordTimecode}
          disabled={!connected || busy}
          onClick={() => onRequestAction(status?.recording ? 'stopRecord' : 'startRecord')}
        />
      </div>
    </section>
  );
}

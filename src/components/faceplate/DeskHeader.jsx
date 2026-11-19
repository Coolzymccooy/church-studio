import { useState } from 'react';
import { isTauri } from '../../lib/platform';
import { getDesktopObsController } from '../../lib/obsControl.js';
import { getDesktopRecorderController } from '../../lib/recorder.js';
import { engineState, gateLamp, headerReadout, obsLamp, recLamp } from '../../lib/faceplateHeader.js';
import { useObsStatus } from '../obs/useObsStatus.js';
import { useRecorderStatus } from '../recorder/useRecorder.js';
import DeskClock from './DeskClock.jsx';
import Glass from './Glass.jsx';
import Key from './Key.jsx';
import Lamp from './Lamp.jsx';
import TwoPressKey from './TwoPressKey.jsx';

/**
 * The desk header plate: engine start and stop (stop needs two presses), engine state, the engine
 * readout, status lamps, the setup key and the clock. Replaces the old status bar.
 */
export default function DeskHeader({
  isLive, isPlayingFile, audioStats, gateClosed, onToggleEngine, onShowSettings,
}) {
  const [busy, setBusy] = useState(false);
  const obs = useObsStatus(isTauri ? getDesktopObsController() : null);
  const [rec] = useRecorderStatus(isTauri ? getDesktopRecorderController() : null);

  const state = engineState(isLive, isPlayingFile);
  const lamps = [gateLamp(isLive || isPlayingFile, gateClosed), obsLamp(obs), recLamp(rec)];

  const runEngine = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await onToggleEngine();
    } finally {
      setBusy(false);
    }
  };

  return (
    <header className="fp-header" aria-label="Desk header">
      <div className="fp-brand"><b>tiwaton</b><span className="fp-kl">studio desk</span></div>

      <div className="fp-grp" role="group" aria-label="Engine">
        {isLive
          ? <TwoPressKey label="stop engine" tone="red" onConfirm={runEngine} disabled={busy}>stop</TwoPressKey>
          : <Key lit={null} onClick={runEngine} disabled={busy} aria-label="start engine">start</Key>}
        <Lamp tone={state.tone}>{busy ? (isLive ? 'stopping' : 'starting') : state.word}</Lamp>
      </div>

      <Glass className="fp-read" role="group" aria-label="Engine readout">
        {headerReadout(audioStats).map((r) => (
          <span key={r.key}><em>{r.key}</em><span className={r.warn ? 'fp-warn' : undefined}>{r.value}</span></span>
        ))}
      </Glass>

      <div className="fp-grp" role="group" aria-label="Status">
        {lamps.map((l) => <Lamp key={l.word.split(' ')[0]} tone={l.tone}>{l.word}</Lamp>)}
      </div>

      <div className="fp-end">
        <Key onClick={onShowSettings}>setup</Key>
        <DeskClock />
      </div>
    </header>
  );
}

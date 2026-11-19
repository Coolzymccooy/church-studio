import { useMemo, useState } from 'react';

import { isTauri } from '../../lib/platform';
import { describeObsError, getDesktopObsController } from '../../lib/obsControl.js';
import { obsLampState, outputKey, stepScene } from '../../lib/obsPlate.js';
import Glass from '../faceplate/Glass.jsx';
import Key from '../faceplate/Key.jsx';
import Lamp from '../faceplate/Lamp.jsx';
import TwoPressKey from '../faceplate/TwoPressKey.jsx';
import { useObsStatus } from '../obs/useObsStatus.js';

function OutputKey({ kind, active, timecode, disabled, onRun }) {
  const legend = outputKey(kind, active, timecode);
  const lit = active ? (kind === 'stream' ? 'green' : 'red') : null;
  const label = active ? `stop ${kind === 'stream' ? 'stream' : 'recording'}` : (kind === 'stream' ? 'go live' : 'start recording');
  return (
    <TwoPressKey label={label} tone={active ? 'red' : 'amber'} lit={lit} className="fp-okey" disabled={disabled} onConfirm={onRun}>
      <span>{legend.text}</span>
      <span className="fp-num fp-okey-tc">{legend.tc}</span>
    </TwoPressKey>
  );
}

function ObsPlateDesktop({ onOpenObs }) {
  const controller = useMemo(() => getDesktopObsController(), []);
  const status = useObsStatus(controller);
  const [error, setError] = useState(null);
  const lamp = obsLampState(status);
  const live = Boolean(status.connected);

  const run = async (fn) => {
    setError(null);
    try {
      await fn();
    } catch (err) {
      setError(describeObsError(err));
    }
  };
  const step = (dir) => {
    const next = stepScene(status.scenes, status.currentScene, dir);
    if (next) run(() => controller.setScene(next));
  };

  return (
    <section className="fp-plate" aria-label="OBS">
      <div className="fp-plate-hd">
        <span>OBS</span>
        <span className="fp-keys" style={{ alignItems: 'center' }}>
          <Lamp tone={lamp.tone}>{lamp.word}</Lamp>
          <Key onClick={onOpenObs} aria-label="OBS settings">set up</Key>
        </span>
      </div>
      <div className="fp-obsrow">
        <Key className="fp-pg" aria-label="Previous OBS scene" disabled={!live || status.scenes.length < 2} onClick={() => step(-1)}>‹</Key>
        <Glass className="fp-win" aria-label="OBS program scene">
          <em>program</em>
          <span className="fp-win-v">{live ? (status.currentScene || 'none') : 'not connected'}</span>
        </Glass>
        <Key className="fp-pg" aria-label="Next OBS scene" disabled={!live || status.scenes.length < 2} onClick={() => step(1)}>›</Key>
      </div>
      <div className="fp-obskeys">
        <OutputKey kind="stream" active={status.streaming} timecode={status.streamTimecode} disabled={!live}
          onRun={() => run(() => (status.streaming ? controller.stopStream(true) : controller.startStream(true)))} />
        <OutputKey kind="record" active={status.recording} timecode={status.recordTimecode} disabled={!live}
          onRun={() => run(() => (status.recording ? controller.stopRecord(true) : controller.startRecord(true)))} />
      </div>
      {(error || status.lastError) && <div className="fp-sub fp-fault" role="alert">{error || status.lastError}</div>}
    </section>
  );
}

/** The OBS plate: connection lamp, program scene with ‹ ›, and two-press go-live and record keys. */
export default function ObsPlate({ onOpenObs }) {
  if (!isTauri) {
    return (
      <section className="fp-plate" aria-label="OBS">
        <div className="fp-plate-hd"><span>OBS</span><Lamp tone="off">desktop app only</Lamp></div>
      </section>
    );
  }
  return <ObsPlateDesktop onOpenObs={onOpenObs} />;
}

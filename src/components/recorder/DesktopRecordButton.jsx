import { useState } from 'react';

import { getDesktopRecorderController, isStopArmed, quickRecordView, stopArmKey } from '../../lib/recorder.js';
import Key from '../faceplate/Key.jsx';
import { useRecorder } from './useRecorder.js';

/**
 * DesktopRecordButton: the centre-bar record key in the desktop app.
 * Starts and stops the native multitrack recorder (every armed strip plus the Stream mix).
 * Stop needs two presses; the arm belongs to one recording only (its `stopArmKey`), so if that
 * recording ends by another route the next one must not stop on a single press.
 */
export default function DesktopRecordButton() {
  const recorder = useRecorder(getDesktopRecorderController());
  const [armedKey, setArmedKey] = useState(null);
  const confirmStop = isStopArmed(recorder.status, armedKey);
  const view = quickRecordView(recorder.status, recorder.error, confirmStop);
  const recording = view.mode === 'recording';

  const onPrimary = async () => {
    if (recording) {
      if (!confirmStop) {
        setArmedKey(stopArmKey(recorder.status));
        return;
      }
      setArmedKey(null);
      await recorder.stop();
      return;
    }
    setArmedKey(null);
    await recorder.start(null);
  };

  const cls = recording && confirmStop ? 'fp-arm-r' : undefined;
  return (
    <div className="fp-keys" style={{ alignItems: 'center' }}>
      <Key
        lit={recording && !confirmStop ? 'red' : null}
        className={cls}
        onClick={onPrimary}
        disabled={recorder.busy || view.disabled}
        title={view.title}
      >
        {view.label}
      </Key>
      {view.canOpenFolder && <Key onClick={recorder.openFolder}>open folder</Key>}
      {view.message && (
        <span className={`fp-sub ${view.messageLevel === 'error' ? 'fp-fault' : ''}`} role={view.messageLevel === 'error' ? 'alert' : undefined}>
          {view.message}
        </span>
      )}
    </div>
  );
}

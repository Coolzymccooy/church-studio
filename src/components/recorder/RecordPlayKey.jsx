import { useCallback, useEffect, useMemo, useState } from 'react';

import InfoTip from '../faceplate/InfoTip.jsx';
import Key from '../faceplate/Key.jsx';
import { createPreviewController, normalizePreviewStatus, previewKeyLabel } from '../../lib/recordPreview.js';

const POLL_MS = 250;

async function tauriInvoke(cmd, args) {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke(cmd, args);
}

/**
 * "play" on the saved row: plays the last recording's Stream mix into the
 * Monitor output (desktop only). The key turns into "stop m:ss" while it plays.
 */
export default function RecordPlayKey({ engineOn }) {
  const controller = useMemo(() => createPreviewController({ invoke: tauriInvoke }), []);
  const [status, setStatus] = useState(() => normalizePreviewStatus(null));
  const [error, setError] = useState(null);

  const run = useCallback(async (call) => {
    try {
      setError(null);
      setStatus(normalizePreviewStatus(await call()));
    } catch (err) {
      setError(typeof err === 'string' ? err : err?.message ?? 'Could not play the recording');
    }
  }, []);

  useEffect(() => {
    if (!status.playing) return undefined;
    const id = setInterval(() => run(() => controller.status()), POLL_MS);
    return () => clearInterval(id);
  }, [status.playing, controller, run]);

  const toggle = () => run(() => (status.playing ? controller.stop() : controller.play()));

  return (
    <>
      <Key onClick={toggle} disabled={!engineOn && !status.playing} lit={status.playing ? 'green' : null}
        pressed={status.playing} aria-label={status.playing ? 'Stop playing the recording' : 'Play the last recording'}
        className="fp-num">
        {previewKeyLabel(status)}
      </Key>
      <InfoTip label="Play">Plays the Stream mix into your monitor. The monitor key must be on.</InfoTip>
      {error && <span role="alert" className="fp-sub fp-fault">{error}</span>}
    </>
  );
}

import React, { useEffect, useMemo, useState } from 'react';

import { InfoTip, Key, Lamp } from './faceplate';
import { isTauri } from '../lib/platform';
import { computerAudioLamp, createComputerAudioClient } from '../lib/computerAudio';

const tauriInvoke = (command, args) => (
  import('@tauri-apps/api/core').then(({ invoke }) => invoke(command, args))
);

function describeError(err) {
  if (typeof err === 'string') return err;
  if (err && typeof err.message === 'string') return err.message;
  return 'Could not change computer audio.';
}

/**
 * On/off key for the computer audio input: what this computer plays
 * (music, video, a browser) as a stereo pair of mixer strips, with no
 * virtual cable. Desktop only.
 */
export default function ComputerAudioToggle() {
  if (!isTauri) return null;
  return <ComputerAudioKey />;
}

function ComputerAudioKey() {
  const client = useMemo(() => createComputerAudioClient(tauriInvoke), []);
  const [settings, setSettings] = useState(null);
  const [error, setError] = useState(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    client.get()
      .then((next) => { if (!cancelled) setSettings(next); })
      .catch((err) => { if (!cancelled) setError(describeError(err)); });
    return () => { cancelled = true; };
  }, [client]);

  const toggle = async () => {
    if (!settings || busy) return;
    setBusy(true);
    setError(null);
    try {
      setSettings(await client.set(!settings.enabled));
    } catch (err) {
      setError(describeError(err));
    } finally {
      setBusy(false);
    }
  };

  const enabled = Boolean(settings?.enabled);
  const lamp = computerAudioLamp(settings, error);

  return (
    <div className="flex items-center gap-2">
      <Key lit={enabled ? 'green' : null} pressed={enabled} disabled={!settings || busy} onClick={toggle}>
        Computer audio
      </Key>
      <Lamp tone={lamp.tone}>{lamp.word}</Lamp>
      <InfoTip label="computer audio">
        Brings what this computer plays (music, video, a browser) into the mixer as the
        Computer L and Computer R strips, after the input channels. No virtual cable needed.
        Takes effect when the engine starts. On a Mac the apps go quiet on their own output and
        are heard through the mixer; macOS asks once for System Audio Recording permission. On
        Windows it records the default output device, which must not be one of this app&apos;s
        outputs, and the apps keep playing there too.
        {error ? ` ${error}` : ''}
        {settings?.saveError ? ` Not saved: ${settings.saveError}` : ''}
      </InfoTip>
    </div>
  );
}

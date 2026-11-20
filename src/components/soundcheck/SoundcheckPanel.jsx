import { useEffect, useRef, useState } from 'react';

import { createSoundcheckController, durationLabel } from '../../lib/soundcheck.js';
import { Glass, InfoTip, Key, Lamp } from '../faceplate/index.js';

const desktop = createSoundcheckController((command, args) => (
  import('@tauri-apps/api/core').then(({ invoke }) => invoke(command, args))
));
const POLL_MS = 150;

async function pickSession() {
  const { open } = await import('@tauri-apps/plugin-dialog');
  return open({ multiple: false, filters: [{ name: 'Session', extensions: ['json'] }] });
}

function useSoundcheck(engineOn, mixer) {
  const [status, setStatus] = useState(null);
  const [strips, setStrips] = useState([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const revision = useRef(0);
  const pending = useRef(false);

  useEffect(() => {
    if (!engineOn) return undefined;
    let active = true;
    let polling = false;
    const refresh = async () => {
      if (pending.current || polling) return;
      polling = true;
      const before = revision.current;
      try {
        const next = await desktop.status();
        if (active && before === revision.current) setStatus(next);
      } catch {
        // A restart can interrupt this request. The next poll refreshes it.
      } finally {
        polling = false;
      }
    };
    void refresh();
    const refreshNames = () => mixer.getState().then((state) => {
      if (active) setStrips(state.strips);
    }).catch(() => {});
    void refreshNames();
    const names = mixer.subscribeStripRenames?.(refreshNames);
    const interval = window.setInterval(refresh, POLL_MS);
    return () => {
      active = false;
      names?.dispose();
      window.clearInterval(interval);
    };
  }, [engineOn, mixer]);

  const run = async (action) => {
    if (pending.current) return false;
    pending.current = true;
    revision.current += 1;
    setBusy(true);
    setError('');
    try {
      await action();
      setStatus(await desktop.status());
      return true;
    } catch (err) {
      setError(err?.message ?? String(err));
      return false;
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };

  return { status: engineOn ? status : null, strips, busy, error, run };
}

export default function SoundcheckPanel({ engineOn, mixer }) {
  const { status, busy, error, run, strips } = useSoundcheck(engineOn, mixer);
  const [position, setPosition] = useState(null);
  const session = status?.session;
  const title = session?.title;
  const clock = position ?? status?.seconds ?? 0;
  const stateLabel = !engineOn ? 'Engine off' : status?.buffering ? 'Buffering'
    : status?.playing ? 'Playing' : session ? 'Stopped' : 'Empty';

  const load = () => run(async () => {
    const path = await pickSession();
    if (!path) return;
    await desktop.load(path);
    setPosition(null);
  });
  const unload = async () => {
    if (await run(desktop.unload)) setPosition(null);
  };
  const locate = async (seconds) => {
    if (await run(() => desktop.locate(seconds))) setPosition(null);
  };
  const actionError = error || status?.error;

  return (
    <>
      <div className="fp-keys items-center">
        <Key disabled={!engineOn || busy} onClick={load}>Load</Key>
        {session && <Key onClick={unload} disabled={busy}>Unload</Key>}
        <Lamp tone={status?.buffering ? 'a' : status?.playing ? 'g' : 'off'}>{stateLabel}</Lamp>
        <InfoTip label="Soundcheck">
          Open a recorded session.json. Choose Playback for each track you want to hear through the desk.
          Playback inputs are silent while stopped. Select Live to bring their microphones back.
        </InfoTip>
      </div>
      {actionError && (
        <div className="fp-keys items-center" role="alert">
          <span className="fp-fault">{status?.error || (session ? 'Command failed' : 'Load failed')}</span>
          <InfoTip label="Soundcheck error">{actionError}</InfoTip>
        </div>
      )}
      {session && (
        <Transport session={session} status={status} title={title} clock={clock} position={position}
          setPosition={setPosition} busy={busy} run={run} locate={locate} />
      )}
      {Boolean(session?.warnings?.length) && (
        <div className="fp-keys items-center" role="status">
          <Lamp tone="a">{session.warnings.length} skipped</Lamp>
          <InfoTip label="Skipped tracks">
            {session.warnings.map(({ name, reason }) => <div key={`${name}:${reason}`}>{name} · {reason}</div>)}
          </InfoTip>
        </div>
      )}
      {session?.tracks.map((track) => {
        const playback = Boolean(status.selected & (1 << track.index));
        const label = strips.find((strip) => strip.index === track.index)?.name ?? `Ch ${track.index + 1}`;
        return (
          <div className="fp-row" key={track.index}>
            <span className="truncate" title={label}>{label}</span>
            <div className="fp-keys items-center">
              <Key lit={playback ? 'sel' : null} pressed={playback} disabled={busy}
                aria-label={`${label} source: ${playback ? 'Playback' : 'Live'}`}
                onClick={() => run(() => desktop.source(track.index, !playback))}>
                {playback ? 'Playback' : 'Live'}
              </Key>
              <InfoTip label={`${label} source`}>Recorded track: {track.name}</InfoTip>
            </div>
          </div>
        );
      })}
    </>
  );
}

function Transport({ session, status, title, clock, position, setPosition, busy, run, locate }) {
  return (
    <>
      <div className="fp-row">
        <span className="truncate" title={title}>{title}</span>
        <InfoTip label="Session details">
          {session.sourceRate.toLocaleString()} Hz. {session.droppedFrames} dropped frames in the recording.
        </InfoTip>
      </div>
      <Glass className="fp-timer" aria-label="Playhead">
        {durationLabel(clock)} / {durationLabel(status.duration)}
      </Glass>
      <div className="fp-keys items-center">
        <Key lit={status.playing ? 'green' : null} disabled={busy || status.playing || Boolean(status.error)}
          onClick={() => run(desktop.play)}>Play</Key>
        <Key disabled={busy || !status.playing} onClick={() => run(desktop.stop)}>Stop</Key>
        <Key lit={status.looping ? 'sel' : null} pressed={status.looping} disabled={busy}
          onClick={() => run(() => desktop.loop(!status.looping))}>Loop</Key>
      </div>
      <input type="range" className="fp-range" min="0" max={status.duration} step="any"
        value={clock} aria-label="Locate time" aria-valuetext={durationLabel(clock)} disabled={busy}
        onChange={(event) => setPosition(Number(event.target.value))}
        onPointerUp={(event) => { if (position !== null) void locate(Number(event.currentTarget.value)); }}
        onPointerCancel={() => setPosition(null)}
        onKeyUp={(event) => {
          if (position !== null && /^(Arrow|Home|End|Page)/.test(event.key)) void locate(Number(event.currentTarget.value));
        }} />
      {session.markers.length > 0 && (
        <select value="" className="fp-field" aria-label="Locate marker" disabled={busy}
          onChange={(event) => locate(session.markers[Number(event.target.value)].timeSeconds)}>
          <option value="" disabled>Marker</option>
          {session.markers.map((marker, i) => (
            <option key={i} value={i}>{durationLabel(marker.timeSeconds)} · {marker.label}</option>
          ))}
        </select>
      )}
    </>
  );
}

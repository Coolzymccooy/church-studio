import { useCallback, useEffect, useMemo, useState } from 'react';

import InfoTip from './faceplate/InfoTip.jsx';
import Key from './faceplate/Key.jsx';
import Lamp from './faceplate/Lamp.jsx';

import { isTauri } from '../lib/platform';
import {
  BUSES, MAX_NET_INPUTS, createNetAudioClient, nextBufferMs, passphraseProblem,
} from '../lib/netAudio';

const STATUS_POLL_MS = 1000;

const tauriInvoke = (command, args) => (
  import('@tauri-apps/api/core').then(({ invoke }) => invoke(command, args))
);

function describeError(err) {
  if (typeof err === 'string') return err;
  if (err && typeof err.message === 'string') return err.message;
  return 'Could not change network audio.';
}

/** The sender lamp: tone and word. */
function sendingLamp(sending) {
  if (sending.connected) return { tone: 'g', text: `${sending.rttMs.toFixed(0)} ms` };
  if (sending.refused === 'full') return { tone: 'r', text: 'room full' };
  if (sending.refused === 'rate') return { tone: 'r', text: 'rate differs' };
  return { tone: 'a', text: 'no answer' };
}

/** The value after `current` in `list` (wrapping). */
function nextIn(list, current) {
  const index = list.indexOf(current);
  return list[(index + 1) % list.length];
}

/**
 * Network audio card: receive remote people as Remote strips and send a bus
 * to another Studio (docs/specs/2026-11-20-network-audio-design.md).
 * Settings apply at the next engine start. Desktop only.
 */
export default function NetworkAudioPanel({ isLive }) {
  if (!isTauri) return null;
  return <NetworkAudioCard isLive={isLive} />;
}

function useLiveStatus(client, isLive) {
  const [live, setLive] = useState(null);
  useEffect(() => {
    if (!isLive) return undefined;
    let cancelled = false;
    const poll = () => {
      client.getStatus()
        .then((status) => { if (!cancelled) setLive(status); })
        .catch(() => { if (!cancelled) setLive(null); });
    };
    poll();
    const timer = setInterval(poll, STATUS_POLL_MS);
    return () => { cancelled = true; clearInterval(timer); };
  }, [client, isLive]);
  return isLive ? live : null;
}

function TextField({ id, label, value, placeholder, disabled, onCommit }) {
  const [draft, setDraft] = useState(value);
  useEffect(() => { setDraft(value); }, [value]);
  return (
    <div>
      <label htmlFor={id} className="fp-label">{label}</label>
      <input
        id={id}
        type="text"
        value={draft}
        placeholder={placeholder}
        disabled={disabled}
        spellCheck={false}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={() => { if (draft !== value) onCommit(draft); }}
        onKeyDown={(event) => { if (event.key === 'Enter') event.currentTarget.blur(); }}
        className="fp-field"
      />
    </div>
  );
}

function PeerRow({ peer }) {
  const detail = peer.connected
    ? `${peer.address} · round trip ${peer.rttMs.toFixed(1)} ms · buffer ${peer.bufferMs.toFixed(0)} ms · jitter ${peer.jitterMs.toFixed(1)} ms · lost ${peer.lostPackets}`
    : 'No one connected';
  const rateWrong = peer.connected && peer.rateMismatchPackets > 0;
  return (
    <div className="fp-row" title={detail}>
      <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, minWidth: 0 }}>
        <span className={peer.connected ? 'fp-led fp-led-g' : 'fp-led'} aria-label={peer.connected ? 'connected' : 'free'} />
        <span style={{ overflow: 'hidden', textOverflow: 'ellipsis' }}>{peer.connected && peer.name ? peer.name : peer.label}</span>
      </span>
      {rateWrong && <span className="fp-sub fp-fault">{peer.senderRate / 1000} kHz</span>}
      {!rateWrong && peer.connected && <span className="fp-sub fp-num">{peer.latencyMs.toFixed(0)} ms</span>}
    </div>
  );
}

function NetworkAudioCard({ isLive }) {
  const client = useMemo(() => createNetAudioClient(tauriInvoke), []);
  const [config, setConfig] = useState(null);
  const [addresses, setAddresses] = useState([]);
  const [notice, setNotice] = useState(null);
  const [busy, setBusy] = useState(false);
  const live = useLiveStatus(client, isLive);

  useEffect(() => {
    let cancelled = false;
    client.get()
      .then((value) => { if (!cancelled) setConfig(value); })
      .catch((err) => { if (!cancelled) setNotice({ error: true, text: describeError(err) }); });
    client.localAddresses()
      .then((list) => { if (!cancelled) setAddresses(list); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [client]);

  const apply = useCallback(async (next) => {
    setBusy(true);
    try {
      const result = await client.set(next);
      setConfig(result.config);
      if (result.saveError) setNotice({ error: true, text: `Not saved: ${result.saveError}` });
      else if (result.problem) setNotice({ error: true, text: result.problem });
      else setNotice(result.restartRequired ? { error: false, text: 'Restart engine to apply' } : null);
    } catch (err) {
      setNotice({ error: true, text: describeError(err) });
    } finally {
      setBusy(false);
    }
  }, [client]);

  const newPassphrase = async () => {
    try {
      const passphrase = await client.newPassphrase();
      if (passphrase) await apply({ ...config, passphrase });
    } catch (err) {
      setNotice({ error: true, text: describeError(err) });
    }
  };

  if (!config) {
    return (
      <section className="fp-mod" aria-label="Network audio">
        <div className="fp-mod-hd"><span>Network audio</span></div>
        <div className="fp-mod-bd">
          {notice && <div className="fp-sub fp-fault" role="alert">{notice.text}</div>}
        </div>
      </section>
    );
  }

  const { receive, send } = config;
  const setReceive = (patch) => apply({ ...config, receive: { ...receive, ...patch } });
  const setSend = (patch) => apply({ ...config, send: { ...send, ...patch } });
  const peers = live?.peers ?? [];
  const anyPeer = peers.some((peer) => peer.connected);
  const sending = live?.sending;
  const problem = passphraseProblem(config);
  const slotSteps = Array.from({ length: MAX_NET_INPUTS }, (_, i) => i + 1);

  return (
    <section className="fp-mod" aria-label="Network audio">
      <div className="fp-mod-hd">
        <span>
          Network audio
          <InfoTip label="Network audio">Remote people arrive as Remote strips, fader down. Both Studios need the same passphrase and 48 kHz. Use on the church network or the tailnet. Applies when the engine restarts.</InfoTip>
        </span>
        <Lamp tone={anyPeer || sending?.connected ? 'g' : 'off'}>{anyPeer || sending?.connected ? 'linked' : 'idle'}</Lamp>
      </div>
      <div className="fp-mod-bd">
        <div className="fp-row">
          <span>Receive</span>
          <Key lit={receive.enabled ? 'sel' : null} pressed={receive.enabled} disabled={busy}
            aria-label="receive from network" onClick={() => setReceive({ enabled: !receive.enabled })} style={{ minWidth: 36 }}>
            {receive.enabled ? 'on' : 'off'}
          </Key>
        </div>
        {receive.enabled && (
          <>
            <div className="fp-row">
              <span>Strips</span>
              <Key disabled={busy} aria-label={`remote strips: ${receive.slots}`}
                onClick={() => setReceive({ slots: nextIn(slotSteps, receive.slots) })}>{receive.slots}</Key>
            </div>
            <div className="fp-row">
              <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
                Buffer <InfoTip label="Buffer">Raise it if a remote voice crackles; lower it for less delay.</InfoTip>
              </span>
              <Key disabled={busy} aria-label={`buffer: ${receive.bufferMs} ms`}
                onClick={() => setReceive({ bufferMs: nextBufferMs(receive.bufferMs) })}>
                <span className="fp-num">{receive.bufferMs} ms</span>
              </Key>
            </div>
            <div className="fp-row">
              <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
                Address <InfoTip label="Address">Give one of these to the remote Studio as its send address.</InfoTip>
              </span>
              <span className="fp-sub fp-num" style={{ textAlign: 'right' }}>
                {addresses.length ? addresses.map((ip) => `${ip}:${receive.port}`).join('  ') : `port ${receive.port}`}
              </span>
            </div>
            {peers.map((peer) => <PeerRow key={peer.strip} peer={peer} />)}
            {live?.error && <div className="fp-sub fp-fault" role="alert">{live.error}</div>}
          </>
        )}

        <div className="fp-row">
          <span>Send</span>
          <Key lit={send.enabled ? 'sel' : null} pressed={send.enabled} disabled={busy}
            aria-label="send to network" onClick={() => setSend({ enabled: !send.enabled })} style={{ minWidth: 36 }}>
            {send.enabled ? 'on' : 'off'}
          </Key>
        </div>
        {send.enabled && (
          <>
            <TextField id="net-send-target" label="To" value={send.target} placeholder="100.75.12.100:47810"
              disabled={busy} onCommit={(target) => setSend({ target })} />
            <div className="fp-row">
              <span>Bus</span>
              <Key disabled={busy} aria-label={`send bus: ${send.bus}`}
                onClick={() => setSend({ bus: nextIn(BUSES, send.bus) })}>{send.bus}</Key>
            </div>
            <div className="fp-row">
              <span>Stereo</span>
              <Key lit={send.stereo ? 'sel' : null} pressed={send.stereo} disabled={busy}
                aria-label="send stereo" onClick={() => setSend({ stereo: !send.stereo })} style={{ minWidth: 36 }}>
                {send.stereo ? 'on' : 'off'}
              </Key>
            </div>
            <TextField id="net-send-name" label="Name" value={send.name} placeholder="this computer"
              disabled={busy} onCommit={(name) => setSend({ name })} />
            {sending && <Lamp tone={sendingLamp(sending).tone}>{sendingLamp(sending).text}</Lamp>}
          </>
        )}

        {(receive.enabled || send.enabled) && (
          <>
            <TextField id="net-passphrase" label="Passphrase" value={config.passphrase}
              disabled={busy} onCommit={(passphrase) => apply({ ...config, passphrase })} />
            <Key disabled={busy} onClick={newPassphrase}>new passphrase</Key>
          </>
        )}

        {receive.enabled && send.enabled && (
          <div className="fp-sub fp-caution" style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
            May echo <InfoTip label="Echo">If the other Studio also sends you its mix, each side hears itself back once its Remote fader is up. Send a monitor mix without the Remote strips to avoid it.</InfoTip>
          </div>
        )}
        {problem && <div className="fp-sub fp-fault" role="alert">{problem}</div>}
        {notice && !problem && (
          <div className={notice.error ? 'fp-sub fp-fault' : 'fp-sub fp-caution'} role={notice.error ? 'alert' : undefined}>{notice.text}</div>
        )}
      </div>
    </section>
  );
}

import { useEffect, useRef, useState } from 'react';
import { Glass, InfoTip, Key, TwoPressKey } from '../faceplate/index.js';
import { pairUrl } from '../../../src-tauri/src/remote_mixer/web/protocol.js';
import { copyText } from '../../lib/copyText.js';
import { addressLabel, countdownLabel, PAIR_SECONDS, seenLabel } from '../../lib/remoteMixerPanel.js';

const ROLES = { monitor1: 'mon 1', monitor2: 'mon 2', monitor3: 'mon 3', desk: 'desk' };
const COPIED_MS = 1500;

/** The listening state: address, then pick a role and pair, then the paired devices. */
export default function RemotePairing({ status, busy, invoke, action }) {
  const [role, setRole] = useState('monitor1');
  const [issued, setIssued] = useState(null);
  const pairing = status.pairing ?? {};
  const active = issued && pairing.state === 'ready';
  const expired = issued && pairing.state === 'expired';

  const pair = () => action(async () => setIssued({ code: await invoke('pair_code', { role }), role }));
  const cancel = () => action(async () => { await invoke('cancel_code'); setIssued(null); });

  return (
    <>
      <Address url={status.url} fingerprint={status.fingerprint} />

      <section className="fp-rm-step" aria-label="Pair a device">
        <div className="fp-rm-hd">
          <span>pair a device</span>
          <InfoTip label="Pairing">Choose what the device may control, then pair. On the device, scan the QR or open the address and type the code. A code works once, for two minutes.</InfoTip>
        </div>
        <div className="fp-rm-roles" role="radiogroup" aria-label="Role">
          {Object.entries(ROLES).map(([id, name]) => (
            <Key key={id} role="radio" aria-checked={role === id} lit={role === id ? 'sel' : null}
              disabled={busy || Boolean(active)} onClick={() => setRole(id)}>{name}</Key>
          ))}
        </div>
        {active ? (
          <PairCode code={issued.code} role={issued.role} seconds={pairing.remainingSeconds}
            url={pairUrl(status.url, issued.code)} busy={busy} cancel={cancel} />
        ) : (
          <Key lit="green" className="fp-key-wide fp-key-tall" disabled={busy} onClick={pair}>
            {expired ? 'new code' : 'pair'} {ROLES[role]}
          </Key>
        )}
        {pairing.lockedClients > 0 && (
          <div className="fp-row">
            <span className="fp-sub fp-caution">{pairing.lockedClients} in backoff</span>
            <InfoTip label="Pairing backoff">
              {pairing.lockedClients === 1 ? 'One address has' : `${pairing.lockedClients} addresses have`} entered wrong codes and must wait before trying again. Other devices can still use the active code.
            </InfoTip>
          </div>
        )}
      </section>

      <section className="fp-rm-step" aria-label="Paired devices">
        <div className="fp-rm-hd">
          <span>paired</span>
          <span className="fp-num fp-sub">{status.devices.length}</span>
        </div>
        {status.devices.length === 0 ? (
          <span className="fp-sub">none yet</span>
        ) : (
          <ul className="fp-rm-devices">
            {status.devices.map((device) => (
              <Device key={device.id} device={device} busy={busy}
                revoke={() => action(() => invoke('revoke', { id: device.id }))} />
            ))}
          </ul>
        )}
      </section>

      <div className="fp-rm-step fp-keys">
        <TwoPressKey label="Stop remote mixer" tone="red" disabled={busy}
          tip="Press twice. Stops listening and disconnects every paired device."
          onConfirm={() => action(async () => { await invoke('stop'); setIssued(null); })}>
          stop
        </TwoPressKey>
      </div>
    </>
  );
}

function Address({ url, fingerprint }) {
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return undefined;
    const timer = window.setTimeout(() => setCopied(false), COPIED_MS);
    return () => window.clearTimeout(timer);
  }, [copied]);
  return (
    <div className="fp-rm-addr">
      <Glass className="fp-rm-url" title={url} aria-label="Remote mixer address">{addressLabel(url)}</Glass>
      <Key aria-label="Copy address" onClick={async () => setCopied(await copyText(url))}>{copied ? 'copied' : 'copy'}</Key>
      <InfoTip label="Certificate fingerprint">
        <span className="break-all">SHA-256 {fingerprint}</span>
      </InfoTip>
    </div>
  );
}

function PairCode({ code, role, seconds, url, busy, cancel }) {
  const left = Math.max(0, Math.min(PAIR_SECONDS, seconds ?? 0));
  return (
    <div className="fp-rm-code">
      <Glass className="fp-rm-digits" role="status" aria-label={`Pairing code ${code.split('').join(' ')} for ${ROLES[role]}`}>
        {code.slice(0, 3)}<span className="fp-rm-gap" />{code.slice(3)}
      </Glass>
      <div className="fp-rm-timer">
        <span className="fp-rm-chip">{ROLES[role]}</span>
        <span className="fp-rm-drain" aria-hidden="true"><i style={{ width: `${(left / PAIR_SECONDS) * 100}%` }} /></span>
        <span className="fp-num">{countdownLabel(left)}</span>
      </div>
      <PairQr url={url} />
      <Key className="fp-key-wide" disabled={busy} onClick={cancel}>cancel</Key>
    </div>
  );
}

function Device({ device, busy, revoke }) {
  const seen = seenLabel(device.seenSeconds);
  return (
    <li className="fp-rm-device">
      <span className={`fp-led ${seen === 'live' ? 'fp-led-g' : ''}`} aria-hidden="true" />
      <span className="fp-rm-name fp-ellipsis" title={device.name}>{device.name}</span>
      <span className="fp-rm-meta">
        <span className="fp-rm-chip">{ROLES[device.role]}</span>
        <span>{seen}</span>
      </span>
      <span className="fp-rm-revoke">
        <TwoPressKey label={`Revoke ${device.name}`} tone="red" disabled={busy}
          tip="Press twice. The device must pair again." onConfirm={revoke}>
          revoke
        </TwoPressKey>
      </span>
    </li>
  );
}

function PairQr({ url }) {
  const canvas = useRef(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let active = true;
    import('qrcode').then(({ default: qr }) => {
      if (active) return qr.toCanvas(canvas.current, url, { width: 184, margin: 2 });
    }).catch(() => { if (active) setFailed(true); });
    return () => { active = false; };
  }, [url]);
  if (failed) return null;
  return <canvas ref={canvas} className="fp-rm-qr" role="img" aria-label="Pair device by QR code" />;
}

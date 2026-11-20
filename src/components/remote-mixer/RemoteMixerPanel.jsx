import { useEffect, useRef, useState } from 'react';
import { Glass, InfoTip, Key, Lamp, TwoPressKey } from '../faceplate/index.js';
import { pairUrl } from '../../../src-tauri/src/remote_mixer/web/protocol.js';

const ROLES = { monitor1: 'Mon 1', monitor2: 'Mon 2', monitor3: 'Mon 3', desk: 'Desk' };
const initial = { ip: '', host: '', port: 4475, certificate: '', privateKey: '' };
const invoke = (command, args) => import('@tauri-apps/api/core')
  .then(({ invoke: native }) => native(`remote_mixer_${command}`, args));

export default function RemoteMixerPanel() {
  const [status, setStatus] = useState(null);
  const [settings, setSettings] = useState(initial);
  const [role, setRole] = useState('monitor1');
  const [code, setCode] = useState(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [tailnet, setTailnet] = useState(null);

  useEffect(() => {
    let active = true;
    invoke('tailnet_address').then((address) => { if (active) setTailnet(address); }).catch(() => {});
    return () => { active = false; };
  }, []);

  useEffect(() => {
    let finished = false;
    let pending = false;
    let hydrated = false;
    const refresh = async () => {
      if (pending) return;
      pending = true;
      try {
        const next = await invoke('status');
        if (!finished) {
          setStatus(next);
          if (!hydrated) {
            hydrated = true;
            if (next.settings) setSettings(next.settings);
          }
        }
      } catch (err) {
        if (!finished) setError(err?.message ?? String(err));
      } finally {
        pending = false;
      }
    };
    void refresh();
    const timer = window.setInterval(refresh, 1000);
    return () => {
      finished = true;
      window.clearInterval(timer);
    };
  }, []);

  const action = async (fn) => {
    if (busy) return;
    setBusy(true);
    setError('');
    try {
      await fn();
      setStatus(await invoke('status'));
    } catch (err) {
      setError(err?.message ?? String(err));
    } finally {
      setBusy(false);
    }
  };

  const pick = (key) => action(async () => {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const path = await open({ multiple: false });
    if (typeof path === 'string') setSettings((prev) => ({ ...prev, [key]: path }));
  });

  const running = Boolean(status?.url);
  const pairing = status?.pairing?.state;
  const activeCode = code && pairing === 'ready';
  const pairingLabel = { ready: 'Ready', expired: 'Expired' }[pairing];
  const blocked = status?.pairing?.lockedClients ?? 0;
  const complete = settings.ip && settings.host && settings.certificate && settings.privateKey && settings.port > 0 && settings.port <= 65535;

  return (
    <>
      <div className="fp-row">
        <Lamp tone={running ? 'g' : 'off'}>{running ? 'Listening' : 'Off'}</Lamp>
        <InfoTip label="Remote mixer">On a LAN, use a certificate trusted by each device and enter its hostname. Compare the SHA-256 fingerprint with the pairing page before entering a code. Each device receives the chosen role.</InfoTip>
      </div>
      {error && (
        <div className="fp-row" role="alert">
          <span>Remote mixer error</span>
          <InfoTip label="Remote mixer error">{error}</InfoTip>
        </div>
      )}
      {running ? (
        <>
          <div className="fp-row">
            <input readOnly className="fp-field" aria-label="Remote mixer address" value={status.url} />
            <InfoTip label="Certificate fingerprint">
              <span className="break-all">SHA-256 {status.fingerprint}</span>
            </InfoTip>
          </div>
          <div className="fp-keys">
            <TwoPressKey label="Stop remote mixer" disabled={busy}
              tip="Press twice to disconnect all paired devices."
              onConfirm={() => action(async () => { await invoke('stop'); setCode(null); })}>
              Stop
            </TwoPressKey>
          </div>
          <div className="fp-row">
            <select value={role} className="fp-field" aria-label="Pairing role" disabled={busy}
              onChange={(event) => setRole(event.target.value)}>
              {Object.entries(ROLES).map(([id, name]) => <option key={id} value={id}>{name}</option>)}
            </select>
            <Key disabled={busy} onClick={() => action(async () => setCode(await invoke('pair_code', { role })))}>Pair code</Key>
          </div>
          {activeCode && (
            <div className="fp-row">
              <Glass className="fp-timer" aria-label="Pairing code">{code}</Glass>
              <Key disabled={busy} onClick={() => action(async () => { await invoke('cancel_code'); setCode(null); })}>Cancel</Key>
            </div>
          )}
          {pairingLabel && (
            <div className="fp-row">
              <Lamp tone={pairing === 'ready' ? 'g' : 'a'}>{pairingLabel}</Lamp>
              {blocked > 0 && <InfoTip label="Pairing backoff">
                {blocked} {blocked === 1 ? 'address is' : 'addresses are'} in backoff. Other devices can still use the active code.
              </InfoTip>}
            </div>
          )}
          {activeCode && <PairQr url={pairUrl(status.url, code)} />}
          {status.devices.map((device) => (
            <div className="fp-row" key={device.id}>
              <span className="fp-ellipsis" title={device.name}>{device.name} · {ROLES[device.role]}</span>
              <Key disabled={busy} aria-label={`Revoke ${device.name}`}
                onClick={() => action(() => invoke('revoke', { id: device.id }))}>Revoke</Key>
            </div>
          ))}
        </>
      ) : (
        <>
          {tailnet && (
            <div className="fp-row">
              <Key disabled={busy} onClick={() => action(async () => {
                setSettings(await invoke('tailnet_certificate', { port: settings.port || initial.port }));
              })}>Tailscale certificate</Key>
              <InfoTip label="Tailscale certificate">Request a trusted certificate for {tailnet.host}. Tailscale HTTPS must be enabled. The hostname is published in certificate transparency logs.</InfoTip>
            </div>
          )}
          <SettingsField name="IP address" value={settings.ip} onChange={(ip) => setSettings((prev) => ({ ...prev, ip }))} />
          <SettingsField name="Hostname" value={settings.host} onChange={(host) => setSettings((prev) => ({ ...prev, host }))} />
          <SettingsField name="Port" value={settings.port} onChange={(port) => setSettings((prev) => ({ ...prev, port: Number(port) }))} numeric />
          <FileKey label="Certificate" chosen={Boolean(settings.certificate)} disabled={busy} pick={() => pick('certificate')} />
          <FileKey label="Private key" chosen={Boolean(settings.privateKey)} disabled={busy} pick={() => pick('privateKey')} />
          <div className="fp-keys">
            <Key disabled={busy || !status || !complete} onClick={() => action(() => invoke('start', { settings }))}>{busy ? 'Starting' : 'Start'}</Key>
          </div>
        </>
      )}
    </>
  );
}

function PairQr({ url }) {
  const canvas = useRef(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let active = true;
    import('qrcode').then(({ default: qr }) => {
      if (active) return qr.toCanvas(canvas.current, url, { width: 156 });
    }).catch(() => { if (active) setFailed(true); });
    return () => { active = false; };
  }, [url]);
  if (failed) return null;
  return <canvas ref={canvas} role="img" aria-label="Pair device by QR code" />;
}

function SettingsField({ name, value, onChange, numeric = false }) {
  return (
    <label className="grid grid-cols-3 items-center gap-2 text-fp-sm">
      <span>{name}</span>
      <input type={numeric ? 'number' : 'text'} min={numeric ? 1 : undefined} max={numeric ? 65535 : undefined}
        value={value} className="fp-field col-span-2" onChange={(event) => onChange(event.target.value)} />
    </label>
  );
}

function FileKey({ label, chosen, disabled, pick }) {
  return (
    <div className="fp-row">
      <span>{label}</span>
      <Key aria-label={`${label}: ${chosen ? 'Selected' : 'Choose'}`} disabled={disabled}
        onClick={pick}>{chosen ? 'Selected' : 'Choose'}</Key>
    </div>
  );
}

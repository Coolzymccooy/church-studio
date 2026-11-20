import { useEffect, useState } from 'react';
import { Glass, InfoTip, Key, Lamp, TwoPressKey } from '../faceplate/index.js';

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

  useEffect(() => {
    let finished = false;
    let pending = false;
    const refresh = async () => {
      if (pending) return;
      pending = true;
      try {
        const next = await invoke('status');
        if (!finished) setStatus(next);
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

  useEffect(() => {
    if (code === null) return undefined;
    const timer = window.setTimeout(() => setCode(null), 120_000);
    return () => window.clearTimeout(timer);
  }, [code]);

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
  const complete = settings.ip && settings.host && settings.certificate && settings.privateKey && settings.port > 0 && settings.port <= 65535;

  return (
    <>
      <div className="fp-row">
        <Lamp tone={running ? 'g' : 'off'}>{running ? 'Listening' : 'Off'}</Lamp>
        <InfoTip label="Remote mixer">Choose the desktop's LAN or Tailscale IPv4 address. Use the certificate's hostname and a certificate your clients trust. Each paired device gets only the role you choose.</InfoTip>
      </div>
      {error && (
        <div className="fp-row" role="alert">
          <span>Remote mixer error</span>
          <InfoTip label="Remote mixer error">{error}</InfoTip>
        </div>
      )}
      {running ? (
        <>
          <input readOnly className="fp-field" aria-label="Remote mixer address" value={status.url} />
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
          {code && (
            <div className="fp-row">
              <Glass className="fp-timer" aria-label="Pairing code">{code}</Glass>
              <Key disabled={busy} onClick={() => action(async () => { await invoke('cancel_code'); setCode(null); })}>Cancel</Key>
            </div>
          )}
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

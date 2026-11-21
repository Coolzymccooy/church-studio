import { useEffect, useState } from 'react';
import { InfoTip, Key, Lamp } from '../faceplate/index.js';
import RemotePairing from './RemotePairing.jsx';

const initial = { ip: '', host: '', port: 4475, certificate: '', privateKey: '' };
const invoke = (command, args) => import('@tauri-apps/api/core')
  .then(({ invoke: native }) => native(`remote_mixer_${command}`, args));

export default function RemoteMixerPanel() {
  const [status, setStatus] = useState(null);
  const [settings, setSettings] = useState(initial);
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
  const complete = settings.ip && settings.host && settings.certificate && settings.privateKey && settings.port > 0 && settings.port <= 65535;

  return (
    <>
      <div className="fp-row">
        <Lamp tone={running ? 'g' : 'off'}>{running ? 'listening' : 'off'}</Lamp>
        <InfoTip label="Remote mixer">Phones and tablets on this network or tailnet mix their own monitor, or run the desk. Start listening, pick a role, pair, then scan the QR or type the code on the device. On a LAN, use a certificate each device trusts and compare the SHA-256 fingerprint with the pairing page.</InfoTip>
      </div>
      {error && (
        <div className="fp-row" role="alert">
          <span className="fp-fault">remote mixer error</span>
          <InfoTip label="Remote mixer error">{error}</InfoTip>
        </div>
      )}
      {running ? (
        <RemotePairing status={status} busy={busy} invoke={invoke} action={action} />
      ) : (
        <>
          {tailnet && (
            <div className="fp-row">
              <Key disabled={busy} onClick={() => action(async () => {
                setSettings(await invoke('tailnet_certificate', { port: settings.port || initial.port }));
              })}>tailscale certificate</Key>
              <InfoTip label="Tailscale certificate">Request a trusted certificate for {tailnet.host}. Tailscale HTTPS must be enabled. The hostname is published in certificate transparency logs.</InfoTip>
            </div>
          )}
          <SettingsField name="IP address" value={settings.ip} onChange={(ip) => setSettings((prev) => ({ ...prev, ip }))} />
          <SettingsField name="Hostname" value={settings.host} onChange={(host) => setSettings((prev) => ({ ...prev, host }))} />
          <SettingsField name="Port" value={settings.port} onChange={(port) => setSettings((prev) => ({ ...prev, port: Number(port) }))} numeric />
          <FileKey label="Certificate" chosen={Boolean(settings.certificate)} disabled={busy} pick={() => pick('certificate')} />
          <FileKey label="Private key" chosen={Boolean(settings.privateKey)} disabled={busy} pick={() => pick('privateKey')} />
          <div className="fp-keys">
            <Key lit="green" className="fp-key-wide fp-key-tall" disabled={busy || !status || !complete} onClick={() => action(() => invoke('start', { settings }))}>{busy ? 'starting' : 'start listening'}</Key>
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
      <Key aria-label={`${label}: ${chosen ? 'chosen' : 'choose'}`} disabled={disabled}
        onClick={pick}>{chosen ? 'chosen' : 'choose'}</Key>
    </div>
  );
}

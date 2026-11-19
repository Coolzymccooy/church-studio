import React, { useId, useState } from 'react';

import { OBS_DEFAULT_HOST, OBS_DEFAULT_PORT, connectionState, parsePort } from '../../lib/obsControl.js';
import InfoTip from '../faceplate/InfoTip.jsx';
import Key from '../faceplate/Key.jsx';
import Lamp from '../faceplate/Lamp.jsx';

const LAMP = {
  connected: { tone: 'g', label: 'connected' },
  connecting: { tone: 'a', label: 'connecting' },
  disconnected: { tone: 'off', label: 'disconnected' },
};

/**
 * Host / port / password, Connect-Disconnect, status lamp and OBS version.
 * Fields are seeded from `config`; the parent remounts this (via `key`) when
 * the saved address changes.
 */
export default function ObsConnectionSection({
  config, status, busy, onConnect, onDisconnect, onClearPassword,
}) {
  const id = useId();
  const [host, setHost] = useState(() => config?.host || OBS_DEFAULT_HOST);
  const [portText, setPortText] = useState(() => String(config?.port || OBS_DEFAULT_PORT));
  const [password, setPassword] = useState('');
  const [fieldError, setFieldError] = useState(null);

  const state = connectionState(status);
  const lamp = LAMP[state];
  const enabled = Boolean(config?.enabled);

  const connect = () => {
    const port = parsePort(portText);
    if (!host.trim()) return setFieldError('Enter the OBS address');
    if (port === null) return setFieldError('Port must be 1–65535 (OBS uses 4455)');
    setFieldError(null);
    // An empty field keeps the saved password.
    onConnect({ host: host.trim(), port, password: password === '' ? null : password });
    setPassword('');
  };

  const message = fieldError || status?.lastError;

  return (
    <section className="fp-mod" aria-label="OBS connection">
      <div className="fp-mod-hd">
        <span>
          Connection
          <InfoTip label="OBS connection">
            In OBS: Tools → WebSocket Server Settings → Enable WebSocket server. “Show Connect Info” shows the password.
          </InfoTip>
        </span>
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }} aria-live="polite">
          <Lamp tone={lamp.tone}>{lamp.label}</Lamp>
          {status?.obsVersion && <span className="fp-sub fp-num">OBS {status.obsVersion}</span>}
        </span>
      </div>
      <div className="fp-mod-bd">
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 90px', gap: 8 }}>
          <div>
            <label className="fp-label" htmlFor={`${id}-host`}>Address</label>
            <input
              id={`${id}-host`}
              className="fp-field"
              value={host}
              disabled={enabled}
              onChange={(e) => setHost(e.target.value)}
            />
          </div>
          <div>
            <label className="fp-label" htmlFor={`${id}-port`}>Port</label>
            <input
              id={`${id}-port`}
              className="fp-field"
              value={portText}
              inputMode="numeric"
              disabled={enabled}
              onChange={(e) => setPortText(e.target.value)}
            />
          </div>
        </div>

        <div>
          <div className="fp-label" style={{ justifyContent: 'space-between' }}>
            <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
              <label htmlFor={`${id}-password`}>Password</label>
              {config?.hasPassword && (
                <InfoTip label="OBS password">Leave blank to keep the saved password.</InfoTip>
              )}
            </span>
            {config?.hasPassword && !enabled && onClearPassword && (
              <Key
                disabled={busy}
                onClick={() => { setPassword(''); onClearPassword(); }}
                aria-label="clear saved password"
              >
                clear saved
              </Key>
            )}
          </div>
          <input
            id={`${id}-password`}
            className="fp-field"
            type="password"
            autoComplete="off"
            value={password}
            disabled={enabled}
            placeholder={config?.hasPassword ? 'saved' : 'none'}
            onChange={(e) => setPassword(e.target.value)}
          />
        </div>

        <div className="fp-row">
          {message ? (
            <span
              className={fieldError ? 'fp-sub fp-caution' : 'fp-sub fp-fault'}
              style={{ whiteSpace: 'normal' }}
              role="status"
            >
              {message}
            </span>
          ) : (
            <span />
          )}
          {enabled ? (
            <Key disabled={busy} onClick={onDisconnect}>disconnect</Key>
          ) : (
            <Key disabled={busy} onClick={connect}>connect</Key>
          )}
        </div>
      </div>
    </section>
  );
}

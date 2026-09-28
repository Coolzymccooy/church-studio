import React, { useState } from 'react';

import { OBS_DEFAULT_HOST, OBS_DEFAULT_PORT, connectionState, parsePort } from '../../lib/obsControl.js';

const DOT = {
  connected: { cls: 'bg-[#00E676]', label: 'Connected' },
  connecting: { cls: 'bg-amber-400 animate-pulse', label: 'Connecting…' },
  disconnected: { cls: 'bg-slate-600', label: 'Disconnected' },
};

const inputCls = 'w-full bg-slate-950 border border-slate-700 rounded-lg px-2.5 py-1.5 text-xs text-slate-200';

/**
 * Host / port / password, Connect-Disconnect, status dot and OBS version.
 * Fields are seeded from `config`; the parent remounts this (via `key`) when
 * the saved address changes.
 */
export default function ObsConnectionSection({ config, status, busy, onConnect, onDisconnect }) {
  const [host, setHost] = useState(() => config?.host || OBS_DEFAULT_HOST);
  const [portText, setPortText] = useState(() => String(config?.port || OBS_DEFAULT_PORT));
  const [password, setPassword] = useState('');
  const [fieldError, setFieldError] = useState(null);

  const state = connectionState(status);
  const dot = DOT[state];
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

  return (
    <section className="space-y-3">
      <div className="flex items-center justify-between">
        <span className="flex items-center gap-2 text-xs text-slate-300" aria-live="polite">
          <span className={`w-2.5 h-2.5 rounded-full ${dot.cls}`} />
          {dot.label}
          {status?.obsVersion && (
            <span className="text-slate-500">· OBS {status.obsVersion}</span>
          )}
        </span>
        {enabled ? (
          <button
            type="button"
            disabled={busy}
            onClick={onDisconnect}
            className="px-3 py-1.5 rounded-lg text-xs font-semibold bg-slate-800 text-slate-200 hover:bg-slate-700 disabled:opacity-50"
          >
            Disconnect
          </button>
        ) : (
          <button
            type="button"
            disabled={busy}
            onClick={connect}
            className="px-3 py-1.5 rounded-lg text-xs font-bold text-black disabled:opacity-50"
            style={{ background: 'var(--accent)' }}
          >
            Connect
          </button>
        )}
      </div>

      <div className="grid grid-cols-[1fr_90px] gap-2">
        <label className="text-[10px] uppercase font-bold text-slate-500">
          Address
          <input className={inputCls} value={host} disabled={enabled} onChange={(e) => setHost(e.target.value)} />
        </label>
        <label className="text-[10px] uppercase font-bold text-slate-500">
          Port
          <input
            className={inputCls}
            value={portText}
            inputMode="numeric"
            disabled={enabled}
            onChange={(e) => setPortText(e.target.value)}
          />
        </label>
      </div>
      <label className="block text-[10px] uppercase font-bold text-slate-500">
        Password
        <input
          className={inputCls}
          type="password"
          autoComplete="off"
          value={password}
          disabled={enabled}
          placeholder={config?.hasPassword ? 'Saved (leave blank to keep)' : 'None'}
          onChange={(e) => setPassword(e.target.value)}
        />
      </label>
      <p className="text-[10px] text-slate-600">
        In OBS: Tools → WebSocket Server Settings → Enable WebSocket server. “Show Connect Info” shows the password.
      </p>

      {(fieldError || status?.lastError) && (
        <p className="text-[11px] text-amber-400" role="status">{fieldError || status.lastError}</p>
      )}
    </section>
  );
}

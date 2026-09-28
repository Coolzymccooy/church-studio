import React, { useCallback, useEffect, useMemo, useState } from 'react';

import { isTauri, openExternal } from '../lib/platform';
import {
  NDI_BUSES,
  NDI_TOOLS_URL,
  NDI_TRADEMARK,
  NDI_URL,
  createNdiClient,
  describeNdiStatus,
  ndiSourceName,
  sanitizeNdiBaseName,
} from '../lib/ndiOutput';

const SENDING_POLL_MS = 2000;

const tauriInvoke = (command, args) => (
  import('@tauri-apps/api/core').then(({ invoke }) => invoke(command, args))
);

function describeError(err) {
  if (typeof err === 'string') return err;
  if (err && typeof err.message === 'string') return err.message;
  return 'Could not change the NDI outputs.';
}

function ExternalLink({ href, children }) {
  return (
    <button
      type="button"
      onClick={() => { openExternal(href).catch(() => {}); }}
      className="text-[var(--accent)] hover:underline"
    >
      {children}
    </button>
  );
}

/**
 * "Send to NDI®" card for the Output Router: runtime status, one toggle per
 * bus, the source base name and a live "Sending" indicator. Desktop only.
 */
export default function NdiOutputPanel({ isLive }) {
  if (!isTauri) return null;
  return <NdiOutputCard isLive={isLive} />;
}

function NdiOutputCard({ isLive }) {
  const client = useMemo(() => createNdiClient(tauriInvoke), []);
  const [status, setStatus] = useState(null);
  const [outputs, setOutputs] = useState(null);
  const [nameDraft, setNameDraft] = useState('');
  const [sending, setSending] = useState([]);
  const [notice, setNotice] = useState(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    Promise.all([client.getStatus(), client.getOutputs()])
      .then(([nextStatus, nextOutputs]) => {
        if (cancelled) return;
        setStatus(nextStatus);
        setOutputs(nextOutputs);
        setNameDraft(nextOutputs.baseName);
      })
      .catch((err) => { if (!cancelled) setNotice({ error: true, text: describeError(err) }); });
    return () => { cancelled = true; };
  }, [client]);

  useEffect(() => {
    if (!isLive) return undefined;
    let cancelled = false;
    const poll = () => {
      client.getSending()
        .then((names) => { if (!cancelled) setSending(names); })
        .catch(() => { if (!cancelled) setSending([]); });
    };
    poll();
    const timer = setInterval(poll, SENDING_POLL_MS);
    return () => { cancelled = true; clearInterval(timer); };
  }, [client, isLive]);

  const apply = useCallback(async (next) => {
    setBusy(true);
    try {
      const result = await client.setOutputs(next);
      setOutputs(result.outputs);
      setNameDraft(result.outputs.baseName);
      setNotice(result.restartRequired
        ? { error: false, text: result.message || 'Restart the engine to apply the NDI outputs.' }
        : null);
    } catch (err) {
      setNotice({ error: true, text: describeError(err) });
    } finally {
      setBusy(false);
    }
  }, [client]);

  const commitName = () => {
    if (!outputs) return;
    const baseName = sanitizeNdiBaseName(nameDraft);
    if (baseName === outputs.baseName) {
      setNameDraft(baseName);
      return;
    }
    apply({ ...outputs, baseName });
  };

  const statusInfo = describeNdiStatus(status);
  const isSending = isLive && sending.length > 0;

  return (
    <div className="rounded-lg border border-slate-800 overflow-hidden" style={{ background: '#080E1F' }}>
      <div className="px-3 py-1.5 border-b border-slate-800 flex items-center justify-between" style={{ background: 'rgba(255,255,255,0.03)' }}>
        <span className="text-[9px] font-bold tracking-[0.1em] text-slate-500 uppercase">Send to NDI®</span>
        <span className={`flex items-center gap-1 text-[8px] font-bold uppercase ${isSending ? 'text-[#00E676]' : 'text-slate-600'}`}>
          <span className={`w-2 h-2 rounded-full ${isSending ? 'bg-[#00E676] animate-pulse' : 'bg-slate-700'}`} />
          {isSending ? 'Sending' : 'Idle'}
        </span>
      </div>
      <div className="p-2.5 space-y-2 text-[9px]">
        <p className={statusInfo.available ? 'text-slate-400' : 'text-amber-400'}>
          {statusInfo.available || !status
            ? statusInfo.label
            : <ExternalLink href={NDI_TOOLS_URL}>{statusInfo.label}</ExternalLink>}
        </p>

        {NDI_BUSES.map(({ key, label }) => (
          <label key={key} className="flex items-center justify-between gap-2 cursor-pointer">
            <span className="text-slate-300 truncate" title={ndiSourceName(outputs?.baseName, label)}>{label}</span>
            <input
              type="checkbox"
              className="accent-[var(--accent)]"
              checked={Boolean(outputs?.[key])}
              disabled={!outputs || busy}
              onChange={(event) => apply({ ...outputs, [key]: event.target.checked })}
            />
          </label>
        ))}

        <div>
          <label htmlFor="ndi-base-name" className="block text-slate-500 mb-0.5">Source name</label>
          <input
            id="ndi-base-name"
            type="text"
            maxLength={80}
            value={nameDraft}
            disabled={!outputs || busy}
            onChange={(event) => setNameDraft(event.target.value)}
            onBlur={commitName}
            onKeyDown={(event) => { if (event.key === 'Enter') event.currentTarget.blur(); }}
            className="w-full bg-slate-950 border border-slate-700 rounded px-1.5 py-1 text-[10px] text-slate-200"
          />
        </div>

        {isSending && (
          <ul className="space-y-0.5 text-[#00E676]">
            {sending.map((name) => <li key={name} className="truncate">● {name}</li>)}
          </ul>
        )}

        {notice && (
          <p className={notice.error ? 'text-[#FF5252]' : 'text-amber-400'}>{notice.text}</p>
        )}

        <p className="text-[8px] text-slate-600 leading-snug">
          {NDI_TRADEMARK}. <ExternalLink href={NDI_URL}>ndi.video</ExternalLink>
        </p>
      </div>
    </div>
  );
}

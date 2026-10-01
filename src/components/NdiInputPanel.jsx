import React, { useCallback, useEffect, useMemo, useState } from 'react';

import { isTauri, openExternal } from '../lib/platform';
import { NDI_TOOLS_URL, NDI_TRADEMARK, NDI_URL, createNdiClient } from '../lib/ndiOutput';
import {
  MAX_NDI_INPUTS,
  createNdiInputClient,
  mergeSourceList,
  receiveAvailableFromStatus,
  toggleNdiSource,
} from '../lib/ndiInput';

const STATUS_POLL_MS = 2000;
/** How soon to ask again while the NDI runtime is still loading. */
const STATUS_RETRY_MS = 500;

const tauriInvoke = (command, args) => (
  import('@tauri-apps/api/core').then(({ invoke }) => invoke(command, args))
);

function describeError(err) {
  if (typeof err === 'string') return err;
  if (err && typeof err.message === 'string') return err.message;
  return 'Could not change the NDI inputs.';
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
 * "Receive from NDI®" card: find sources on the network, pick up to four,
 * and see each one's live state. Each picked source becomes a mixer strip
 * at the next engine start. Desktop only.
 */
export default function NdiInputPanel({ isLive }) {
  if (!isTauri) return null;
  return <NdiInputCard isLive={isLive} />;
}

function useLiveInputs(client, isLive) {
  const [live, setLive] = useState([]);
  useEffect(() => {
    if (!isLive) return undefined;
    let cancelled = false;
    const poll = () => {
      client.getInputStatus()
        .then((inputs) => { if (!cancelled) setLive(inputs); })
        .catch(() => { if (!cancelled) setLive([]); });
    };
    poll();
    const timer = setInterval(poll, STATUS_POLL_MS);
    return () => { cancelled = true; clearInterval(timer); };
  }, [client, isLive]);
  return isLive ? live : [];
}

function NdiInputCard({ isLive }) {
  const client = useMemo(() => createNdiInputClient(tauriInvoke), []);
  const runtimeClient = useMemo(() => createNdiClient(tauriInvoke), []);
  const [selected, setSelected] = useState(null);
  const [found, setFound] = useState([]);
  const [searching, setSearching] = useState(false);
  const [receiveAvailable, setReceiveAvailable] = useState(true);
  const [notice, setNotice] = useState(null);
  const [busy, setBusy] = useState(false);
  const live = useLiveInputs(client, isLive);

  const refresh = useCallback(async () => {
    setSearching(true);
    try {
      setFound(await client.listSources());
    } catch (err) {
      setNotice({ error: true, text: describeError(err) });
    } finally {
      setSearching(false);
    }
  }, [client]);

  useEffect(() => {
    let cancelled = false;
    let retry = null;
    client.getInputs()
      .then((inputs) => { if (!cancelled) setSelected(inputs.sources); })
      .catch((err) => { if (!cancelled) setNotice({ error: true, text: describeError(err) }); });
    // The runtime loads in the background; ask again until it is done.
    const pollStatus = () => {
      runtimeClient.getStatus()
        .then((status) => {
          if (cancelled) return;
          const available = receiveAvailableFromStatus(status);
          if (available === null) {
            retry = setTimeout(pollStatus, STATUS_RETRY_MS);
          } else {
            setReceiveAvailable(available);
          }
        })
        .catch(() => {});
    };
    pollStatus();
    return () => { cancelled = true; clearTimeout(retry); };
  }, [client, runtimeClient]);

  const toggle = async (name, checked) => {
    const next = toggleNdiSource(selected, name, checked);
    setBusy(true);
    try {
      const result = await client.setInputs(next);
      setSelected(result.inputs.sources);
      if (result.saveError) {
        setNotice({ error: true, text: `Applied, but not saved: ${result.saveError}` });
      } else {
        setNotice(result.restartRequired
          ? { error: false, text: result.message || 'Restart the engine to apply the NDI inputs.' }
          : null);
      }
    } catch (err) {
      setNotice({ error: true, text: describeError(err) });
    } finally {
      setBusy(false);
    }
  };

  const rows = mergeSourceList(found, selected || []);
  const full = (selected?.length ?? 0) >= MAX_NDI_INPUTS;
  const liveBySource = new Map(live.map((input) => [input.source, input]));
  const anyConnected = live.some((input) => input.connected);

  return (
    <div className="rounded-lg border border-slate-800 overflow-hidden" style={{ background: '#080E1F' }}>
      <div className="px-3 py-1.5 border-b border-slate-800 flex items-center justify-between" style={{ background: 'rgba(255,255,255,0.03)' }}>
        <span className="text-[9px] font-bold tracking-[0.1em] text-slate-500 uppercase">Receive from NDI®</span>
        <span className={`flex items-center gap-1 text-[8px] font-bold uppercase ${anyConnected ? 'text-[#00E676]' : 'text-slate-600'}`}>
          <span className={`w-2 h-2 rounded-full ${anyConnected ? 'bg-[#00E676] animate-pulse' : 'bg-slate-700'}`} />
          {anyConnected ? 'Receiving' : 'Idle'}
        </span>
      </div>
      <div className="p-2.5 space-y-2 text-[9px]">
        {!receiveAvailable && (
          <p className="text-amber-400">
            This NDI runtime cannot receive. <ExternalLink href={NDI_TOOLS_URL}>Get NDI Tools</ExternalLink>
          </p>
        )}

        <div className="flex items-center justify-between gap-2">
          <span className="text-slate-500">Sources (up to {MAX_NDI_INPUTS})</span>
          <button
            type="button"
            onClick={refresh}
            disabled={searching || busy}
            className="px-1.5 py-0.5 rounded border border-slate-700 text-slate-300 hover:text-[var(--accent)] disabled:opacity-40"
          >
            {searching ? 'Searching…' : 'Refresh'}
          </button>
        </div>

        {rows.length === 0 && !searching && (
          <p className="text-slate-600">Press Refresh to look for sources on the network.</p>
        )}

        {rows.map((row) => {
          const checked = Boolean(selected?.includes(row.name));
          const status = liveBySource.get(row.name);
          const disabled = !selected || busy || row.own || (!checked && full);
          const hint = row.own
            ? 'This is one of this app’s own outputs; receiving it would feed back.'
            : row.name;
          return (
            <label key={row.name} className="flex items-center justify-between gap-2 cursor-pointer" title={hint}>
              <span className={`truncate ${row.offline || row.own ? 'text-slate-600' : 'text-slate-300'}`}>
                {status && (
                  <span className={status.connected ? 'text-[#00E676]' : 'text-[#FF5252]'}>● </span>
                )}
                {row.name}
                {row.offline && ' (not found)'}
                {row.own && ' (this app)'}
                {status?.connected && ` · ${status.fillMs} ms`}
              </span>
              <input
                type="checkbox"
                className="accent-[var(--accent)]"
                checked={checked}
                disabled={disabled}
                onChange={(event) => toggle(row.name, event.target.checked)}
              />
            </label>
          );
        })}

        <p className="text-slate-600 leading-snug">
          Each source becomes a mixer strip after the input channels, with its fader down.
          Changes apply when the engine restarts.
        </p>

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

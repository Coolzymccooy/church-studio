import { useCallback, useEffect, useMemo, useState } from 'react';

import InfoTip from './faceplate/InfoTip.jsx';
import Key from './faceplate/Key.jsx';
import Lamp from './faceplate/Lamp.jsx';

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
      className="fp-link"
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
    <section className="fp-mod" aria-label="Receive from NDI">
      <div className="fp-mod-hd">
        <span>
          Receive NDI®
          <InfoTip label="Receive NDI">Each source becomes a mixer strip after the input channels, fader down. Applies when the engine restarts. Up to {MAX_NDI_INPUTS}.</InfoTip>
        </span>
        <Lamp tone={anyConnected ? 'g' : 'off'}>{anyConnected ? 'receiving' : 'idle'}</Lamp>
      </div>
      <div className="fp-mod-bd">
        {!receiveAvailable && (
          <div className="fp-sub fp-caution">
            Cannot receive. <ExternalLink href={NDI_TOOLS_URL}>Get NDI Tools</ExternalLink>
          </div>
        )}

        <div className="fp-row">
          <span>Sources</span>
          <Key onClick={refresh} disabled={searching || busy}>{searching ? 'searching' : 'refresh'}</Key>
        </div>

        {rows.length === 0 && !searching && <span className="fp-sub">none found</span>}

        {rows.map((row) => {
          const checked = Boolean(selected?.includes(row.name));
          const status = liveBySource.get(row.name);
          const disabled = !selected || busy || row.own || (!checked && full);
          const note = row.own ? 'this app' : row.offline ? 'not found' : status?.connected ? `${status.fillMs} ms` : null;
          return (
            <div key={row.name} className="fp-row" title={row.own ? 'One of this app’s own outputs; receiving it would feed back.' : row.name}>
              <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, minWidth: 0 }}>
                {status && <span className={`fp-led ${status.connected ? 'fp-led-g' : 'fp-led-r'}`} aria-label={status.connected ? 'connected' : 'not connected'} />}
                <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', color: row.offline || row.own ? 'var(--fp-muted)' : undefined }}>{row.name}</span>
              </span>
              <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }}>
                {note && <span className="fp-sub fp-num">{note}</span>}
                <Key lit={checked ? 'sel' : null} pressed={checked} disabled={disabled} aria-label={`receive ${row.name}`}
                  onClick={() => toggle(row.name, !checked)} style={{ minWidth: 36 }}>
                  {checked ? 'on' : 'off'}
                </Key>
              </span>
            </div>
          );
        })}

        {notice && (
          <div className={notice.error ? 'fp-sub fp-fault' : 'fp-sub fp-caution'} role={notice.error ? 'alert' : undefined}>{notice.text}</div>
        )}

        <div className="fp-sub">
          {NDI_TRADEMARK}. <ExternalLink href={NDI_URL}>ndi.video</ExternalLink>
        </div>
      </div>
    </section>
  );
}

import { useCallback, useEffect, useMemo, useState } from 'react';

import Lamp from './faceplate/Lamp.jsx';
import ToggleRow from './desk/ToggleRow.jsx';

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
const STATUS_RETRY_MS = 500;

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
      className="fp-link"
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
    let retry = null;
    // The runtime loads in the background; ask again until it is done.
    const pollStatus = () => {
      client.getStatus()
        .then((nextStatus) => {
          if (cancelled) return;
          setStatus(nextStatus);
          if (nextStatus?.loading) retry = setTimeout(pollStatus, STATUS_RETRY_MS);
        })
        .catch(() => { if (!cancelled) setStatus({ available: false }); });
    };
    pollStatus();
    client.getOutputs()
      .then((nextOutputs) => {
        if (cancelled) return;
        setOutputs(nextOutputs);
        setNameDraft(nextOutputs.baseName);
      })
      .catch((err) => { if (!cancelled) setNotice({ error: true, text: describeError(err) }); });
    return () => { cancelled = true; clearTimeout(retry); };
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
      if (result.saveError) {
        setNotice({ error: true, text: `Applied, but not saved: ${result.saveError}` });
      } else {
        setNotice(result.restartRequired
          ? { error: false, text: result.message || 'Restart the engine to apply the NDI outputs.' }
          : null);
      }
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
    <section className="fp-mod" aria-label="Send to NDI">
      <div className="fp-mod-hd">
        <span>Send to NDI®</span>
        <Lamp tone={isSending ? 'g' : 'off'}>{isSending ? 'sending' : 'idle'}</Lamp>
      </div>
      <div className="fp-mod-bd">
        <div className={`fp-sub ${statusInfo.available ? '' : 'fp-caution'}`}>
          {statusInfo.available || !status || status.loading
            ? statusInfo.label
            : <ExternalLink href={NDI_TOOLS_URL}>{statusInfo.label}</ExternalLink>}
        </div>

        {NDI_BUSES.map(({ key, label }) => (
          <ToggleRow
            key={key}
            label={label}
            tip={`Sends as "${ndiSourceName(outputs?.baseName, label)}".`}
            tone="sel"
            on={Boolean(outputs?.[key])}
            disabled={!outputs || busy}
            onToggle={() => apply({ ...outputs, [key]: !outputs[key] })}
          />
        ))}

        <div>
          <label htmlFor="ndi-base-name" className="fp-label">Source name</label>
          <input
            id="ndi-base-name"
            type="text"
            maxLength={80}
            value={nameDraft}
            disabled={!outputs || busy}
            onChange={(event) => setNameDraft(event.target.value)}
            onBlur={commitName}
            onKeyDown={(event) => { if (event.key === 'Enter') event.currentTarget.blur(); }}
            className="fp-field"
          />
        </div>

        {isSending && sending.map((name) => (
          <Lamp key={name} tone="g">{name}</Lamp>
        ))}

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

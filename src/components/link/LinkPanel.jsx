import { useEffect, useId, useMemo, useState } from 'react';

import { isTauri } from '../../lib/platform';
import { formatTimeAgo } from '../../lib/linkControl.js';
import InfoTip from '../faceplate/InfoTip.jsx';
import Key from '../faceplate/Key.jsx';
import Lamp from '../faceplate/Lamp.jsx';
import TwoPressKey from '../faceplate/TwoPressKey.jsx';
import { createTauriLinkController, useLinkPanel } from './useLinkPanel.js';
import LinkRulesTable from './LinkRulesTable.jsx';
import LinkActivityLog from './LinkActivityLog.jsx';

const INLINE = { display: 'inline-flex', alignItems: 'center', gap: 6 };

function CopyField({ label, value, secret = false, tip = null }) {
  const [copied, setCopied] = useState(false);
  const [revealed, setRevealed] = useState(!secret);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  };
  return (
    <div>
      <div className="fp-label">
        {label}
        {tip && <InfoTip label={label}>{tip}</InfoTip>}
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
        <code
          className="fp-field"
          style={{
            display: 'block', flex: 1, lineHeight: '26px', overflow: 'hidden',
            textOverflow: 'ellipsis', whiteSpace: 'nowrap', cursor: 'pointer',
          }}
          onClick={() => setRevealed(true)}
          title={revealed ? value : 'Click to reveal'}
        >
          {revealed ? value : '•'.repeat(24)}
        </code>
        <Key onClick={copy} disabled={!value} aria-label={`copy ${label}`} style={{ minWidth: 48 }}>
          {copied ? 'copied' : 'copy'}
        </Key>
      </div>
    </div>
  );
}

function Module({ title, tip = null, end = null, children }) {
  return (
    <section className="fp-mod" aria-label={title}>
      <div className="fp-mod-hd">
        <span>
          {title}
          {tip && <InfoTip label={title}>{tip}</InfoTip>}
        </span>
        {end}
      </div>
      <div className="fp-mod-bd">{children}</div>
    </section>
  );
}

function AutomationModule({ config, link }) {
  const id = useId();
  const paused = Boolean(config.paused);
  return (
    <Module title="Automation" tip="While paused, Lumina events are logged but no scene is loaded.">
      <div className="fp-row">
        <span style={INLINE}>
          <input
            id={`${id}-listen`}
            type="checkbox"
            className="fp-check"
            checked={Boolean(config.enabled)}
            onChange={(e) => link.setEnabled(e.target.checked)}
          />
          <label htmlFor={`${id}-listen`}>Listen for Lumina</label>
        </span>
        <span style={INLINE}>
          {paused && <Lamp tone="a">paused</Lamp>}
          <Key
            lit={paused ? 'amber' : null}
            pressed={paused}
            onClick={() => link.setPaused(!paused)}
            aria-label="pause automation"
            style={{ minWidth: 56 }}
          >
            {paused ? 'resume' : 'pause'}
          </Key>
        </span>
      </div>
    </Module>
  );
}

function LinkPanelBody({ link }) {
  const { config, entries, scenes, lastEventAt, error, now } = link;
  if (!config) {
    return (
      <p className={error ? 'fp-sub fp-fault' : 'fp-sub'} style={{ margin: 0 }} role={error ? 'alert' : undefined}>
        {error ?? 'loading'}
      </p>
    );
  }
  return (
    <>
      {error && <p className="fp-sub fp-fault" style={{ margin: 0 }} role="alert">{error}</p>}
      {config.serverError && <p className="fp-sub fp-fault" style={{ margin: 0 }} role="alert">{config.serverError}</p>}

      <Module title="Connect Lumina" tip="In Lumina: Connect → TIWATON Studio → paste the URL and token.">
        <CopyField label="Endpoint URL" value={config.url ?? (config.enabled ? '' : 'Link is off')} />
        <CopyField
          label="Token"
          value={config.token}
          secret
          tip="A new token disconnects Lumina until you paste it there."
        />
        <div className="fp-row">
          <span>
            Last event <span className="fp-num fp-sub">{formatTimeAgo(lastEventAt, now)}</span>
          </span>
          <TwoPressKey label="new token" onConfirm={link.regenerateToken} />
        </div>
      </Module>

      <AutomationModule config={config} link={link} />

      <Module title="Rules" tip="First match wins. Changes apply when you save.">
        <LinkRulesTable
          key={JSON.stringify(config.rules)}
          rules={config.rules}
          scenes={scenes}
          onSave={link.saveRules}
        />
      </Module>

      <Module title="Activity">
        <LinkActivityLog entries={entries} now={now} onUndo={link.undo} />
      </Module>
    </>
  );
}

function DesktopLinkPanel({ open }) {
  const controller = useMemo(() => createTauriLinkController(), []);
  const link = useLinkPanel(controller, open);
  return <LinkPanelBody link={link} />;
}

/** Tools → Lumina Link…: endpoint, token, rules and the activity log. */
export default function LinkPanel({ open, onClose }) {
  useEffect(() => {
    if (!open) return undefined;
    const onKey = (e) => { if (e.key === 'Escape') onClose(); };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;
  return (
    <>
      <div className="fp-scrim" aria-hidden="true" />
      <div className="fp-dialog-wrap">
        <div
          className="fp-dialog"
          style={{ width: 'min(680px, 100%)' }}
          role="dialog"
          aria-modal="true"
          aria-label="Lumina Link"
        >
          <div className="fp-dialog-hd">
            <span>Lumina Link</span>
            <Key onClick={onClose} aria-label="close Lumina Link">close</Key>
          </div>
          <div className="fp-dialog-bd">
            {isTauri ? (
              <DesktopLinkPanel open={open} />
            ) : (
              <div className="fp-row">
                <span style={{ ...INLINE, overflow: 'visible' }}>
                  Desktop app only
                  <InfoTip label="Lumina Link">
                    Lumina Link runs in the TIWATON AI Studio desktop app. It listens on this computer for
                    Lumina Presenter&apos;s live events (lyrics, scripture, countdown) and loads the matching
                    mixer scene. Open the desktop app to set it up.
                  </InfoTip>
                </span>
              </div>
            )}
          </div>
        </div>
      </div>
    </>
  );
}

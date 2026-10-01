import { useMemo, useState } from 'react';
import { Check, Copy, Link2, RefreshCw, X } from 'lucide-react';

import { isTauri } from '../../lib/platform';
import { formatTimeAgo } from '../../lib/linkControl.js';
import { createTauriLinkController, useLinkPanel } from './useLinkPanel.js';
import LinkRulesTable from './LinkRulesTable.jsx';
import LinkActivityLog from './LinkActivityLog.jsx';

function CopyField({ label, value, secret = false }) {
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
      <div className="text-[9px] uppercase tracking-wider text-slate-500 mb-0.5">{label}</div>
      <div className="flex items-center gap-1">
        <code
          className="flex-1 truncate rounded bg-slate-900 border border-slate-700 px-2 py-1 text-[10px] text-slate-200 cursor-pointer"
          onClick={() => setRevealed(true)}
          title={revealed ? value : 'Click to reveal'}
        >
          {revealed ? value : '•'.repeat(24)}
        </code>
        <button
          type="button"
          onClick={copy}
          disabled={!value}
          className="flex items-center gap-1 px-2 py-1 rounded border border-slate-700 text-[10px] text-slate-300 hover:text-white disabled:opacity-40"
        >
          {copied ? <Check size={11} /> : <Copy size={11} />} {copied ? 'Copied' : 'Copy'}
        </button>
      </div>
    </div>
  );
}

function Section({ title, children }) {
  return (
    <section className="space-y-2">
      <h3 className="text-[10px] font-bold uppercase tracking-wider text-slate-400">{title}</h3>
      {children}
    </section>
  );
}

function LinkPanelBody({ link }) {
  const { config, entries, scenes, lastEventAt, error, now } = link;
  if (!config) {
    return <p className="text-[11px] text-slate-500">{error ?? 'Loading Tiwaton Link…'}</p>;
  }
  const paused = Boolean(config.paused);
  return (
    <div className="space-y-5">
      {error && <p className="text-[10px] text-red-400">{error}</p>}
      {config.serverError && <p className="text-[10px] text-red-400">{config.serverError}</p>}

      <Section title="Connect Lumina">
        <p className="text-[10px] text-slate-500">
          In Lumina: Connect → TIWATON Studio → paste the URL and token.
        </p>
        <CopyField label="Endpoint URL" value={config.url ?? (config.enabled ? '' : 'Link is off')} />
        <CopyField label="Token" value={config.token} secret />
        <div className="flex items-center justify-between text-[10px]">
          <span className="text-slate-500">
            Last event from Lumina: <span className="text-slate-300">{formatTimeAgo(lastEventAt, now)}</span>
          </span>
          <button
            type="button"
            onClick={link.regenerateToken}
            className="flex items-center gap-1 text-slate-500 hover:text-white"
            title="Lumina will need the new token"
          >
            <RefreshCw size={10} /> New token
          </button>
        </div>
      </Section>

      <Section title="Automation">
        <div className="flex items-center gap-3">
          <label className="flex items-center gap-2 text-[11px] text-slate-300">
            <input
              type="checkbox"
              checked={Boolean(config.enabled)}
              onChange={(e) => link.setEnabled(e.target.checked)}
            />
            Listen for Lumina
          </label>
          <button
            type="button"
            onClick={() => link.setPaused(!paused)}
            aria-pressed={paused}
            className={`ml-auto px-4 py-2 rounded-lg text-[12px] font-bold border transition-colors ${
              paused
                ? 'bg-amber-500 text-black border-amber-400'
                : 'bg-slate-900 text-slate-200 border-slate-600 hover:border-amber-500'
            }`}
          >
            {paused ? 'Automation paused — resume' : 'Pause automation'}
          </button>
        </div>
        {paused && (
          <p className="text-[10px] text-amber-400">
            Held: Lumina events are logged but no scene is loaded.
          </p>
        )}
      </Section>

      <Section title="Rules (first match wins)">
        <LinkRulesTable
          key={JSON.stringify(config.rules)}
          rules={config.rules}
          scenes={scenes}
          onSave={link.saveRules}
        />
      </Section>

      <Section title="Activity">
        <LinkActivityLog entries={entries} now={now} onUndo={link.undo} />
      </Section>
    </div>
  );
}

function DesktopLinkPanel({ open }) {
  const controller = useMemo(() => createTauriLinkController(), []);
  const link = useLinkPanel(controller, open);
  return <LinkPanelBody link={link} />;
}

/** Tools → Lumina Link…: endpoint, token, rules and the activity log. */
export default function LinkPanel({ open, onClose }) {
  if (!open) return null;
  return (
    <div className="fixed inset-0 bg-black/70 z-50 flex items-center justify-center backdrop-blur-sm p-4">
      <div
        className="w-full max-w-2xl max-h-[90vh] overflow-y-auto rounded-2xl border border-slate-700 shadow-2xl p-5"
        style={{ background: '#0D1428' }}
        role="dialog"
        aria-label="Lumina Link"
      >
        <div className="flex items-center justify-between border-b border-slate-800 pb-3 mb-4">
          <h2 className="flex items-center gap-2 text-sm font-bold text-slate-100">
            <Link2 size={14} className="text-[var(--accent)]" /> Lumina Link
          </h2>
          <button type="button" onClick={onClose} className="text-slate-500 hover:text-white" aria-label="Close">
            <X size={16} />
          </button>
        </div>
        {isTauri ? (
          <DesktopLinkPanel open={open} />
        ) : (
          <p className="text-[11px] text-slate-400 leading-relaxed">
            Lumina Link runs in the TIWATON AI Studio desktop app. It listens on this
            computer for Lumina Presenter&apos;s live events (lyrics, scripture, countdown)
            and loads the matching mixer scene. Open the desktop app to set it up.
          </p>
        )}
      </div>
    </div>
  );
}

import { useEffect, useState } from 'react';
import { CHECKLIST, QUICK_FIXES, SETUP_STEPS } from '../lib/helpContent.js';
import Key from './faceplate/Key.jsx';

const TABS = [
  { id: 'fixes', label: 'quick fixes' },
  { id: 'setup', label: 'setup guide' },
  { id: 'checklist', label: 'checklist' },
];

function Checklist() {
  const [checked, setChecked] = useState({});
  const groups = [...new Set(CHECKLIST.map((entry) => entry.group))];
  const done = CHECKLIST.filter((entry) => checked[entry.step]).length;
  const toggle = (step) => setChecked((prev) => ({ ...prev, [step]: !prev[step] }));

  return (
    <div className="fp-read-text" style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
      <div className="fp-row">
        <span className="fp-num">{done} / {CHECKLIST.length} done</span>
        <Key onClick={() => setChecked({})}>clear</Key>
      </div>
      {groups.map((group) => (
        <fieldset key={group} style={{ border: 0, margin: 0, padding: 0, display: 'flex', flexDirection: 'column', gap: 6 }}>
          <legend className="fp-label">{group}</legend>
          {CHECKLIST.filter((entry) => entry.group === group).map(({ step }) => (
            <label key={step} style={{ display: 'flex', gap: 8, alignItems: 'flex-start', cursor: 'pointer' }}>
              <input type="checkbox" className="fp-check" checked={Boolean(checked[step])} onChange={() => toggle(step)} style={{ marginTop: 4 }} />
              <span style={checked[step] ? { color: 'var(--fp-muted)', textDecoration: 'line-through' } : undefined}>{step}</span>
            </label>
          ))}
        </fieldset>
      ))}
    </div>
  );
}

/** The help drawer, opened from the header help key. Escape or the scrim closes it. */
export default function HelpCorner({ open, onClose }) {
  const [tab, setTab] = useState('fixes');

  useEffect(() => {
    if (!open) return undefined;
    const onKey = (e) => { if (e.key === 'Escape') onClose(); };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <>
      <div className="fp-scrim" onClick={onClose} aria-hidden="true" />
      <aside className="fp-drawer" role="dialog" aria-modal="true" aria-label="Help">
        <div className="fp-drawer-hd">
          <span>Help</span>
          <Key onClick={onClose} aria-label="close help">close</Key>
        </div>
        <div className="fp-bar" role="tablist" aria-label="Help sections">
          {TABS.map((t) => (
            <Key key={t.id} role="tab" aria-selected={tab === t.id} lit={tab === t.id ? 'sel' : null} onClick={() => setTab(t.id)}>
              {t.label}
            </Key>
          ))}
        </div>
        <div className="fp-drawer-bd">
          {tab === 'fixes' && (
            <div className="fp-read-text">
              {QUICK_FIXES.map((fix) => (
                <details key={fix.title}>
                  <summary>{fix.title}</summary>
                  <p>{fix.content}</p>
                </details>
              ))}
            </div>
          )}
          {tab === 'setup' && SETUP_STEPS.map((step) => (
            <section key={step.num} className="fp-mod">
              <div className="fp-mod-hd"><span>{step.num}. {step.title}</span></div>
              <div className="fp-mod-bd fp-read-text">
                <ul>{step.items.map((item) => <li key={item}>{item}</li>)}</ul>
              </div>
            </section>
          ))}
          {tab === 'checklist' && <Checklist />}
        </div>
      </aside>
    </>
  );
}

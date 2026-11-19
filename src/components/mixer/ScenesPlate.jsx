import { useState } from 'react';

import { pageOf } from '../../lib/mixerBanks.js';
import { validateSceneName } from '../../lib/mixerEngine.js';
import Glass from '../faceplate/Glass.jsx';
import InfoTip from '../faceplate/InfoTip.jsx';
import Key from '../faceplate/Key.jsx';
import TwoPressKey from '../faceplate/TwoPressKey.jsx';

const PAGE = 5;
const SOURCE_WORD = { desk: 'from the desk', obs: 'from OBS', lumina: 'from Lumina', link: 'from Lumina link' };

function SceneKey({ n, name, current, deleting, onLoad, onDelete }) {
  const body = (
    <>
      <span className="fp-skey-n">{n}</span>
      <span className="fp-skey-l">{name}</span>
    </>
  );
  if (deleting) {
    return <TwoPressKey label={`delete scene ${name}`} tone="red" className="fp-skey" onConfirm={() => onDelete(name)}>{body}</TwoPressKey>;
  }
  return (
    <TwoPressKey label={`load scene ${name}`} lit={current ? 'sel' : null} className="fp-skey" onConfirm={() => onLoad(name)}>
      {body}
    </TwoPressKey>
  );
}

function StoreRow({ onSave, onDone }) {
  const [name, setName] = useState('');
  const [error, setError] = useState(null);
  const save = async () => {
    const trimmed = name.trim();
    const problem = validateSceneName(trimmed);
    if (problem) {
      setError(problem);
      return;
    }
    if (await onSave(trimmed)) onDone();
  };
  return (
    <div className="fp-recrow">
      <input
        value={name}
        autoFocus
        onChange={(e) => { setName(e.target.value); setError(null); }}
        onKeyDown={(e) => { if (e.key === 'Enter') save(); if (e.key === 'Escape') onDone(); }}
        placeholder="Scene name"
        maxLength={40}
        aria-label="New scene name"
        className="fp-field"
        style={{ flex: 1 }}
      />
      <Key lit={name.trim() ? 'sel' : null} onClick={save} disabled={!name.trim()}>save</Key>
      {error && <span className="fp-sub fp-fault" role="alert">{error}</span>}
    </div>
  );
}

/**
 * The scenes plate: what is loaded now and who loaded it, a numbered grid of scene keys (two
 * presses to load), "store" to save the current mix, and a delete mode (two red presses).
 */
export default function ScenesPlate({ scenes, current, onLoad, onSave, onDelete }) {
  const [page, setPage] = useState(0);
  const [mode, setMode] = useState(null); // null | 'store' | 'del'
  const view = pageOf(scenes, page, PAGE);
  const slots = Array.from({ length: PAGE }, (_, i) => view.items[i] ?? null);

  return (
    <section className="fp-plate" aria-label="Scenes">
      <div className="fp-plate-hd">
        <span>
          Scenes
          <InfoTip label="Scenes">A scene stores the whole mix. Press a scene twice to load it. Lumina and the OBS scene link can load scenes too.</InfoTip>
        </span>
        <span className="fp-keys">
          <Key lit={mode === 'store' ? 'sel' : null} pressed={mode === 'store'} onClick={() => setMode((m) => (m === 'store' ? null : 'store'))}>store</Key>
          <Key lit={mode === 'del' ? 'red' : null} pressed={mode === 'del'} disabled={scenes.length === 0}
            onClick={() => setMode((m) => (m === 'del' ? null : 'del'))}>del</Key>
        </span>
      </div>

      <Glass className="fp-win" aria-label="Current scene">
        <em>now</em>
        <span className="fp-win-v">{current?.name ?? 'none'}</span>
        {current && <span className="fp-glass-dim">{SOURCE_WORD[current.source] ?? ''}</span>}
      </Glass>

      <div className="fp-sgrid" role="group" aria-label={mode === 'del' ? 'Delete a scene' : 'Load a scene'}>
        <Key className="fp-pg" aria-label="Previous scenes" disabled={!view.hasPrev} onClick={() => setPage(view.page - 1)}>‹</Key>
        {slots.map((name, i) => (name
          ? <SceneKey key={name} n={view.start + i + 1} name={name} current={current?.name === name}
              deleting={mode === 'del'} onLoad={onLoad} onDelete={(s) => { onDelete(s); setMode(null); }} />
          : <span key={`empty-${i}`} className="fp-skey fp-skey-empty" aria-hidden="true"><span className="fp-skey-n">{view.start + i + 1}</span></span>))}
        <Key className="fp-pg" aria-label="More scenes" disabled={!view.hasNext} onClick={() => setPage(view.page + 1)}>›</Key>
      </div>

      {mode === 'store' && <StoreRow onSave={onSave} onDone={() => setMode(null)} />}
    </section>
  );
}

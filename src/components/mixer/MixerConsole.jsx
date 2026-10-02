import { useState } from 'react';
import { Save, Trash2 } from 'lucide-react';

import { validateSceneName } from '../../lib/mixerEngine.js';
import { useMixer } from './useMixer.js';
import ChannelStrip from './ChannelStrip.jsx';
import BusStrip from './BusStrip.jsx';
import RecordPanel from '../recorder/RecordPanel.jsx';

const BUS_LABELS = { main: 'Main', stream: 'Stream', monitor: 'Monitor' };

/**
 * MixerConsole — scene bar, a horizontally scrolling row of channel strips,
 * and the Main/Stream/Monitor bus strips pinned on the right. Driven by
 * useMixer(controller), which works identically for the real Tauri
 * controller and the in-memory demo controller.
 */
export default function MixerConsole({ controller, demoBanner }) {
  const {
    state, loading, toast, dismissToast, metersRef,
    setStripParam, setStripBool, renameStrip,
    setBusParam, setBusBool,
    saveScene, loadScene, deleteScene,
  } = useMixer(controller);

  const [sceneName, setSceneName] = useState('');
  const [sceneNameError, setSceneNameError] = useState(null);
  const [pendingLoad, setPendingLoad] = useState(null);
  const [pendingDelete, setPendingDelete] = useState(null);

  if (loading || !state) {
    return (
      <div className="flex-1 flex items-center justify-center text-slate-500 text-[11px]" style={{ background: '#050A1C' }}>
        Loading mixer…
      </div>
    );
  }

  const handleSceneClick = (name) => {
    if (pendingLoad === name) {
      loadScene(name);
      setPendingLoad(null);
    } else {
      setPendingLoad(name);
      setPendingDelete(null);
    }
  };

  const handleSaveScene = async () => {
    const trimmed = sceneName.trim();
    const error = validateSceneName(trimmed);
    if (error) {
      setSceneNameError(error);
      return;
    }
    const ok = await saveScene(trimmed);
    if (ok) {
      setSceneName('');
      setSceneNameError(null);
    }
  };

  const handleDeleteScene = (name) => {
    if (pendingDelete === name) {
      deleteScene(name);
      setPendingDelete(null);
    } else {
      setPendingDelete(name);
      setPendingLoad(null);
    }
  };

  return (
    <div className="relative flex-1 flex flex-col min-h-0" style={{ background: '#050A1C' }}>
      {demoBanner && (
        <div
          className="px-3 py-1.5 text-center text-[9px] font-bold tracking-wide"
          style={{ background: 'rgba(255,184,0,0.12)', color: '#FFB800', borderBottom: '1px solid rgba(255,184,0,0.3)' }}
        >
          {demoBanner}
        </div>
      )}

      {/* Scene bar */}
      <div className="h-10 shrink-0 border-b border-slate-800 flex items-center gap-2 px-3" style={{ background: '#0D1428' }}>
        <span className="text-[8px] font-bold uppercase tracking-wider text-slate-500 shrink-0">Scenes</span>
        <div className="flex items-center gap-1 overflow-x-auto flex-1" style={{ scrollbarWidth: 'none' }}>
          {(state.scenes || []).length === 0 && (
            <span className="text-[9px] text-slate-600">No scenes saved yet.</span>
          )}
          {(state.scenes || []).map((name) => (
            <div key={name} className="flex items-center gap-0.5 shrink-0">
              <button
                type="button"
                onClick={() => handleSceneClick(name)}
                className="px-2 py-1 rounded text-[9px] font-semibold border transition-all whitespace-nowrap"
                style={pendingLoad === name
                  ? { background: 'var(--accent)', borderColor: 'var(--accent)', color: '#fff' }
                  : { background: 'transparent', borderColor: '#1e293b', color: '#94a3b8' }}
              >
                {pendingLoad === name ? `Load "${name}"?` : name}
              </button>
              <button
                type="button"
                onClick={() => handleDeleteScene(name)}
                title={`Delete scene ${name}`}
                aria-label={`Delete scene ${name}`}
                className="p-1 rounded border"
                style={pendingDelete === name
                  ? { background: '#FF5252', borderColor: '#FF5252', color: '#fff' }
                  : { background: 'transparent', borderColor: '#1e293b', color: '#64748b' }}
              >
                <Trash2 size={10} />
              </button>
            </div>
          ))}
        </div>
        <div className="flex items-center gap-1 shrink-0">
          <input
            value={sceneName}
            onChange={(e) => { setSceneName(e.target.value); setSceneNameError(null); }}
            onKeyDown={(e) => { if (e.key === 'Enter') handleSaveScene(); }}
            placeholder="Save as…"
            maxLength={40}
            aria-label="New scene name"
            className="w-28 px-2 py-1 rounded text-[9px] border border-slate-700 outline-none focus:border-[var(--accent)]"
            style={{ background: '#050A1C', color: 'white' }}
          />
          <button
            type="button"
            onClick={handleSaveScene}
            disabled={!sceneName.trim()}
            className="p-1.5 rounded border text-[9px] disabled:opacity-40"
            style={{ background: 'rgba(var(--accent-rgb),0.12)', borderColor: 'rgba(var(--accent-rgb),0.4)', color: 'var(--accent)' }}
            title="Save current mix as a scene"
            aria-label="Save current mix as a scene"
          >
            <Save size={12} />
          </button>
        </div>
      </div>
      {sceneNameError && (
        <div className="px-3 py-1 text-[9px] text-[#FF5252]">{sceneNameError}</div>
      )}

      <RecordPanel strips={state.strips} engineRunning={Boolean(state.running)} />

      {/* Strips */}
      <div className="flex-1 flex overflow-hidden">
        <div className="flex-1 flex gap-2 p-2 overflow-x-auto">
          {state.strips.map((strip) => (
            <ChannelStrip
              key={strip.index}
              strip={strip}
              metersRef={metersRef}
              onSetParam={(key, value) => setStripParam(strip.index, key, value)}
              onSetBool={(key, value) => setStripBool(strip.index, key, value)}
              onRename={(name) => renameStrip(strip.index, name)}
            />
          ))}
        </div>
        <div className="flex gap-2 p-2 border-l border-slate-800 shrink-0 overflow-y-auto" style={{ background: '#080E1F' }}>
          {state.buses.map((bus) => (
            <BusStrip
              key={bus.id}
              bus={bus}
              label={BUS_LABELS[bus.id] || bus.id}
              metersRef={metersRef}
              onSetParam={(key, value) => setBusParam(bus.id, key, value)}
              onSetBool={(key, value) => setBusBool(bus.id, key, value)}
            />
          ))}
        </div>
      </div>

      {toast && (
        <div
          role="alert"
          className="absolute bottom-4 left-1/2 -translate-x-1/2 px-4 py-2 rounded-lg border text-[10px] font-semibold shadow-2xl z-30 flex items-center gap-3"
          style={{ background: '#0D1428', borderColor: 'rgba(255,82,82,0.5)', color: '#FF5252' }}
        >
          {toast}
          <button type="button" onClick={dismissToast} aria-label="Dismiss error" className="text-slate-500 hover:text-white">✕</button>
        </div>
      )}
    </div>
  );
}

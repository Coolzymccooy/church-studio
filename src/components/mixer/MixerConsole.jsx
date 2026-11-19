import { useEffect, useState } from 'react';

import { validateSceneName } from '../../lib/mixerEngine.js';
import { nextStripSize, readStripSize, stripScale, stripSizeLabel } from '../../lib/stripScale.js';
import InfoTip from '../faceplate/InfoTip.jsx';
import Key from '../faceplate/Key.jsx';
import Lamp from '../faceplate/Lamp.jsx';
import TwoPressKey from '../faceplate/TwoPressKey.jsx';
import { ScaleContext } from '../faceplate/scaleContext.js';
import RecordPanel from '../recorder/RecordPanel.jsx';
import BusStrip from './BusStrip.jsx';
import ChannelStrip from './ChannelStrip.jsx';
import { useMixer } from './useMixer.js';

const SIZE_KEY = 'tiwaton_strip_size';

function loadSize() {
  try {
    return readStripSize(window.localStorage.getItem(SIZE_KEY));
  } catch {
    return 'fit';
  }
}

/** Strip size setting plus the height of the strip area, for "fit". */
function useStripSize() {
  const [size, setSize] = useState(loadSize);
  const [height, setHeight] = useState(0);
  // A callback ref: the strip area mounts only after the mixer has loaded.
  const [area, areaRef] = useState(null);

  useEffect(() => {
    if (!area || typeof ResizeObserver === 'undefined') return undefined;
    const ro = new ResizeObserver(([entry]) => setHeight(entry.contentRect.height));
    ro.observe(area);
    return () => ro.disconnect();
  }, [area]);

  const cycle = () => setSize((s) => {
    const next = nextStripSize(s);
    try {
      window.localStorage.setItem(SIZE_KEY, next);
    } catch {
      // Storage can be blocked; the size still applies for this session.
    }
    return next;
  });

  return { size, scale: stripScale(size, height), areaRef, cycle };
}

function ScenesBar({ scenes, onLoad, onDelete, onSave, sizeLabel, onCycleSize }) {
  const [name, setName] = useState('');
  const [error, setError] = useState(null);

  const save = async () => {
    const trimmed = name.trim();
    const problem = validateSceneName(trimmed);
    if (problem) {
      setError(problem);
      return;
    }
    if (await onSave(trimmed)) {
      setName('');
      setError(null);
    }
  };

  return (
    <>
      <div className="fp-bar" role="group" aria-label="Scenes">
        <span className="fp-label" style={{ margin: 0 }}>
          Scenes <InfoTip label="Scenes">Press a scene twice to load it. Delete also needs two presses.</InfoTip>
        </span>
        <div className="fp-keys fp-scenes" style={{ flexWrap: 'nowrap' }}>
          {scenes.length === 0 && <span className="fp-sub">none saved</span>}
          {scenes.map((scene) => (
            <span key={scene} className="fp-keys" style={{ flexWrap: 'nowrap' }}>
              <TwoPressKey label={`load scene ${scene}`} onConfirm={() => onLoad(scene)}>{scene}</TwoPressKey>
              <TwoPressKey label={`delete scene ${scene}`} tone="red" onConfirm={() => onDelete(scene)}>del</TwoPressKey>
            </span>
          ))}
        </div>
        <div className="fp-bar-end">
          <input
            value={name}
            onChange={(e) => { setName(e.target.value); setError(null); }}
            onKeyDown={(e) => { if (e.key === 'Enter') save(); }}
            placeholder="Scene name"
            maxLength={40}
            aria-label="New scene name"
            className="fp-field"
            style={{ width: 140 }}
          />
          <Key onClick={save} disabled={!name.trim()}>store</Key>
          <Key onClick={onCycleSize} aria-label={`strip ${sizeLabel}`}>{sizeLabel}</Key>
        </div>
      </div>
      {error && <div className="fp-banner fp-fault" role="alert" style={{ justifyContent: 'flex-start' }}>{error}</div>}
    </>
  );
}

/**
 * MixerConsole: the scenes bar, the record plate, a horizontally scrolling row of channel strips
 * and the Main, Stream and Monitor buses on the right. The strips scale up to fill the height
 * ("size fit") or to a fixed size. Driven by useMixer(controller), which works the same for the
 * Tauri controller and the in-memory demo controller.
 */
export default function MixerConsole({ controller, demoBanner }) {
  const {
    state, loading, toast, dismissToast, metersRef,
    setStripParam, setStripBool, renameStrip,
    setBusParam, setBusBool,
    saveScene, loadScene, deleteScene,
  } = useMixer(controller);
  const { size, scale, areaRef, cycle } = useStripSize();

  if (loading || !state) {
    return <div className="flex-1 grid place-items-center fp-sub">loading mixer</div>;
  }

  return (
    <div className="relative flex-1 flex flex-col min-h-0">
      {demoBanner && (
        <div className="fp-banner">
          <Lamp tone="a">demo mixer</Lamp>
          <InfoTip label="Demo mixer">{demoBanner}</InfoTip>
        </div>
      )}

      <ScenesBar
        scenes={state.scenes || []}
        onLoad={loadScene}
        onDelete={deleteScene}
        onSave={saveScene}
        sizeLabel={stripSizeLabel(size)}
        onCycleSize={cycle}
      />

      <RecordPanel strips={state.strips} engineRunning={Boolean(state.running)} />

      <div ref={areaRef} className="flex-1 flex overflow-hidden min-h-0">
        <ScaleContext.Provider value={scale}>
          <div className="fp-desk-scale" style={{ zoom: scale }}>
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
            <div className="fp-buses" role="group" aria-label="Buses">
              {state.buses.map((bus) => (
                <BusStrip
                  key={bus.id}
                  bus={bus}
                  metersRef={metersRef}
                  onSetParam={(key, value) => setBusParam(bus.id, key, value)}
                  onSetBool={(key, value) => setBusBool(bus.id, key, value)}
                />
              ))}
            </div>
          </div>
        </ScaleContext.Provider>
      </div>

      {toast && (
        <div role="alert" className="fp-toast">
          <span className="fp-led fp-led-r" aria-hidden="true" />
          {toast}
          <Key onClick={dismissToast} aria-label="Dismiss error">dismiss</Key>
        </div>
      )}
    </div>
  );
}

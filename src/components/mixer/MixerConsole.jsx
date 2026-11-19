import { useEffect, useState } from 'react';

import { bankStrips, pickBank, stripBanks } from '../../lib/mixerBanks.js';
import { nextStripSize, readStripSize, stripScale, stripSizeLabel } from '../../lib/stripScale.js';
import InfoTip from '../faceplate/InfoTip.jsx';
import Key from '../faceplate/Key.jsx';
import Lamp from '../faceplate/Lamp.jsx';
import { ScaleContext } from '../faceplate/scaleContext.js';
import RecordPanel from '../recorder/RecordPanel.jsx';
import BusStrip from './BusStrip.jsx';
import ObsPlate from './ObsPlate.jsx';
import ScenesPlate from './ScenesPlate.jsx';
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

function BankRow({ banks, bank, onBank, stripCount, demoBanner, sizeLabel, onCycleSize }) {
  return (
    <div className="fp-bar" role="group" aria-label="Strip banks">
      {banks.length > 1 && (
        <div className="fp-keys" role="tablist" aria-label="Banks">
          {banks.map((b) => (
            <Key key={b.id} role="tab" aria-selected={b.id === bank.id} lit={b.id === bank.id ? 'sel' : null} onClick={() => onBank(b.id)}>
              {b.label}
            </Key>
          ))}
        </div>
      )}
      <span className="fp-num fp-sub">{stripCount} {stripCount === 1 ? 'strip' : 'strips'}</span>
      {demoBanner && (
        <span className="fp-keys" style={{ alignItems: 'center' }}>
          <Lamp tone="a">demo mixer</Lamp>
          <InfoTip label="Demo mixer">{demoBanner}</InfoTip>
        </span>
      )}
      <div className="fp-bar-end">
        <Key onClick={onCycleSize} aria-label={`strip ${sizeLabel}`}>{sizeLabel}</Key>
      </div>
    </div>
  );
}

/**
 * MixerConsole: bank keys, a horizontally scrolling row of channel strips with the Main, Stream
 * and Monitor buses on the right, and a footer of scenes, OBS and recorder plates. The strips scale up to fill the height
 * ("size fit") or to a fixed size. Driven by useMixer(controller), which works the same for the
 * Tauri controller and the in-memory demo controller.
 */
export default function MixerConsole({ controller, demoBanner, onOpenObs }) {
  const {
    state, loading, toast, dismissToast, metersRef, currentScene,
    setStripParam, setStripBool, renameStrip,
    setBusParam, setBusBool,
    saveScene, loadScene, deleteScene,
  } = useMixer(controller);
  const { size, scale, areaRef, cycle } = useStripSize();
  const [bankId, setBankId] = useState('all');

  if (loading || !state) {
    return <div className="flex-1 grid place-items-center fp-sub">loading mixer</div>;
  }

  const banks = stripBanks(state.strips.length, state.ndi_strips ?? 0);
  const bank = pickBank(banks, bankId);
  const shown = bankStrips(state.strips, bank);

  return (
    <div className="relative flex-1 flex flex-col min-h-0">
      <BankRow
        banks={banks}
        bank={bank}
        onBank={setBankId}
        stripCount={state.strips.length}
        demoBanner={demoBanner}
        sizeLabel={stripSizeLabel(size)}
        onCycleSize={cycle}
      />

      <div ref={areaRef} className="flex-1 flex overflow-hidden min-h-0">
        <ScaleContext.Provider value={scale}>
          <div className="fp-desk-scale" style={{ zoom: scale }}>
            <div className="flex-1 flex gap-2 p-2 overflow-x-auto">
              {shown.map((strip) => (
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

      <footer className="fp-foot" aria-label="Scenes, OBS and recorder">
        <ScenesPlate scenes={state.scenes || []} current={currentScene} onLoad={loadScene} onSave={saveScene} onDelete={deleteScene} />
        <ObsPlate onOpenObs={onOpenObs} />
        <RecordPanel strips={state.strips} engineRunning={Boolean(state.running)} />
      </footer>

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

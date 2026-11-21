import { AudioWaveform } from 'lucide-react';
import { useEffect, useState } from 'react';

import { bankStrips, pickBank, stripBanks, stripCounts } from '../../lib/mixerBanks.js';
import { EXTRA_MONITORS, activeExtraMonitors } from '../../lib/faceplateStrip.js';
import { nextStripSize, readStripSize, stripRowPx, stripScale, stripSizeLabel } from '../../lib/stripScale.js';
import InfoTip from '../faceplate/InfoTip.jsx';
import Key from '../faceplate/Key.jsx';
import Lamp from '../faceplate/Lamp.jsx';
import { ScaleContext } from '../faceplate/scaleContext.js';
import RecordPanel from '../recorder/RecordPanel.jsx';
import { createTapTempo } from '../../lib/mixerFx.js';
import FxReturnStrip from './FxReturnStrip.jsx';
import BusStrip from './BusStrip.jsx';
import ObsPlate from './ObsPlate.jsx';
import ScenesPlate from './ScenesPlate.jsx';
import ChannelStrip from './ChannelStrip.jsx';
import { useMixer } from './useMixer.js';
import DcaBank from './DcaBank.jsx';

const SIZE_KEY = 'tiwaton_strip_size';
const isExtraMonitor = (id) => EXTRA_MONITORS.some((m) => m.bus === id);

function loadSize() {
  try {
    return readStripSize(window.localStorage.getItem(SIZE_KEY));
  } catch {
    return 'fit';
  }
}

/** Strip size setting plus the height of the strip area, for "fit". */
function useStripSize(rowPx) {
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

  return { size, scale: stripScale(size, height, rowPx), areaRef, cycle };
}

function BankRow({ banks, bank, onBank, stripCount, demoBanner, sizeLabel, onCycleSize, groupKey }) {
  return (
    <div className="fp-bar" role="group" aria-label="Strip banks">
      {banks.length > 1 && (
        <div className="fp-keys" role="tablist" aria-label="Banks">
          {banks.map((b) => (
            <Key key={b.id} className={b.id === "fx" ? "inline-flex items-center gap-1" : ""} role="tab" aria-selected={b.id === bank.id} lit={b.id === bank.id ? 'sel' : null} onClick={() => onBank(b.id)}>
              {b.id === 'fx' && <AudioWaveform size={14} strokeWidth={1.75} aria-hidden="true" />}
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
      {groupKey}
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
export default function MixerConsole({ controller, refreshKey, demoBanner, onOpenObs, soundcheckSignal }) {
  const [showGroups, setShowGroups] = useState(false);
  const {
    setGroup,
    state, loading, toast, dismissToast, metersRef, currentScene,
    setStripParam, setStripBool, renameStrip,
    setBusParam, setBusBool, setFxControl, setFaderTouch,
    saveScene, loadScene, deleteScene,
  } = useMixer(controller, refreshKey);
  const [tapTempo] = useState(createTapTempo);
  const extraMonitors = activeExtraMonitors(state?.buses);
  const { size, scale, areaRef, cycle } = useStripSize(stripRowPx(state?.strips, extraMonitors.length));
  const [bankId, selectBank] = useState('all');
  const setBankId = (id) => {
    selectBank(id);
    setShowGroups(false);
  };

  if (loading || !state) {
    return <div className="flex-1 grid place-items-center fp-sub">loading mixer</div>;
  }

  // Start a mon 2 / mon 3 mix from mon 1: every channel's send copied across.
  const copyMon1Into = (busId) => {
    const target = EXTRA_MONITORS.find((m) => m.bus === busId);
    if (!target) return;
    state.strips.forEach((s) => setStripParam(s.index, target.send, s.send_monitor_db));
  };

  const counts = stripCounts(state);
  const inputBanks = stripBanks(counts.total, counts.ndi, counts.computer);
  const returns = state.fxReturns ?? [];
  const banks = returns.length ? [...inputBanks, { id: 'fx', label: 'FX' }] : inputBanks;
  const bank = pickBank(banks, bankId);
  const fxBank = bank.id === 'fx';
  const shown = fxBank ? [] : bankStrips(state.strips, bank);
  const tempo = (bpm) => {
    tapTempo.reset();
    setFxControl('fx_b', 'tempo_bpm', bpm);
  };
  const tap = () => {
    const bpm = tapTempo(performance.now());
    if (bpm !== null) setFxControl('fx_b', 'tempo_bpm', bpm);
  };

  return (
    <div className="relative flex-1 flex flex-col min-h-0">
      <BankRow
        banks={banks}
        bank={bank}
        onBank={setBankId}
        stripCount={fxBank ? returns.length : state.strips.length}
        demoBanner={demoBanner}
        sizeLabel={stripSizeLabel(size)}
        onCycleSize={cycle}
        groupKey={state.groups && <Key lit={showGroups ? 'sel' : null} pressed={showGroups}
          onClick={() => setShowGroups((open) => !open)}>Groups</Key>}
      />

      <div ref={areaRef} className="flex-1 flex overflow-hidden min-h-0">
        <ScaleContext.Provider value={scale}>
          <div className="fp-desk-scale" style={{ zoom: scale }} data-group-bank={showGroups || undefined}>
            <div className="flex-1 flex gap-2 p-2 overflow-x-auto">
              {fxBank && returns.map((ret, index) => (
                <FxReturnStrip key={ret.id} ret={ret} index={index} extraMonitors={extraMonitors} metersRef={metersRef}
                  onSet={(key, value) => setFxControl(ret.id, key, value)} onFaderTouch={(down) => setFaderTouch(`bus:${ret.id}`, down)} onTempo={tempo} onTap={tap} />
              ))}
              {shown.map((strip) => (
                <ChannelStrip
                  key={strip.index}
                  strip={strip}
                  groups={state.groups}
                  groupLabels={state.groupLabels}
                  metersRef={metersRef}
                  onSetParam={(key, value) => setStripParam(strip.index, key, value)}
                  onSetBool={(key, value) => setStripBool(strip.index, key, value)}
                  playbackTone={soundcheckSignal?.selected & (1 << strip.index) ? (soundcheckSignal.playing ? 'g' : 'a') : null}
                  onFaderTouch={(down) => setFaderTouch(`strip:${strip.index}`, down)}
                  onRename={(name) => renameStrip(strip.index, name)}
                  extraMonitors={extraMonitors}
                />
              ))}
            </div>
            {showGroups && state.groups && <DcaBank groups={state.groups} labels={state.groupLabels} strips={state.strips} metersRef={metersRef} onChange={setGroup}
                onFaderTouch={(index, down) => setFaderTouch(`dca:${index}`, down)} />}
            <div className="fp-buses" role="group" aria-label="Buses">
              {state.buses.filter((bus) => !isExtraMonitor(bus.id) || extraMonitors.some((m) => m.bus === bus.id)).map((bus) => (
                <BusStrip
                  key={bus.id}
                  bus={bus}
                  metersRef={metersRef}
                  onSetParam={(key, value) => setBusParam(bus.id, key, value)}
                  onSetBool={(key, value) => setBusBool(bus.id, key, value)}
                  onFaderTouch={(down) => setFaderTouch(`bus:${bus.id}`, down)}
                  onCopyFromMon1={isExtraMonitor(bus.id) ? () => copyMon1Into(bus.id) : null}
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

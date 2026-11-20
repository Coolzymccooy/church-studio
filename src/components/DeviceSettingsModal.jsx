import { useCallback, useEffect, useRef, useState } from 'react';

import {
  clampPanel, draftChanges, draftFrom, isDirty, readPanelPos, rowStatus,
} from '../lib/setupDraft.js';
import { isTauri } from '../lib/platform';
import ComputerAudioToggle from './ComputerAudioToggle';
import InfoTip from './faceplate/InfoTip.jsx';
import Key from './faceplate/Key.jsx';
import Lamp from './faceplate/Lamp.jsx';
import LevelBar from './faceplate/LevelBar.jsx';
import TwoPressKey from './faceplate/TwoPressKey.jsx';
import useMeterFeed, { busPeakDb, inputPeakDb } from './desk/useMeterFeed.js';

const POS_KEY = 'tiwaton_setup_pos';
const PANEL_W = 360;
const VIRTUAL_KEYWORDS = ['vb-cable', 'vb cable', 'blackhole', 'black hole', 'loopback', 'virtual'];
const STATUS_LAMP = { on: ['g', 'on'], idle: ['off', 'idle'], 'not set': ['off', 'not set'], unsaved: ['a', 'unsaved'] };

const deviceOptions = (devices, prefix) => devices.map((device) => (
  <option key={device.deviceId} value={device.deviceId}>
    {device.label || `${prefix} ${device.deviceId.slice(0, 5)}`}
  </option>
));

function loadPos() {
  try {
    return readPanelPos(window.localStorage.getItem(POS_KEY));
  } catch {
    return null;
  }
}

function savePos(pos) {
  try {
    window.localStorage.setItem(POS_KEY, JSON.stringify(pos));
  } catch {
    // Storage can be blocked; the position still holds for this session.
  }
}

/** One device: label, ⓘ, status lamp, the select, and a live level bar while it is on. */
function DeviceRow({ id, label, tip, status, value, onChange, getDb, children }) {
  const [tone, word] = STATUS_LAMP[status];
  return (
    <div className="fp-setup-row">
      <div className="fp-row">
        <label className="fp-label" htmlFor={id} style={{ margin: 0 }}>
          {label}
          {tip && <InfoTip label={label}>{tip}</InfoTip>}
        </label>
        <Lamp tone={tone}>{word}</Lamp>
      </div>
      <select id={id} className="fp-field" value={value} onChange={(event) => onChange(event.target.value)}>
        {children}
      </select>
      {isTauri && <LevelBar getDb={status === 'on' ? getDb : () => -96} label={`${label} level`} />}
    </div>
  );
}

/** Drag by the header; the panel stays inside the window and remembers where it was left. */
function useDragPanel(panelRef) {
  const [pos, setPos] = useState(() => loadPos() ?? { x: window.innerWidth - PANEL_W - 16, y: 64 });
  const drag = useRef(null);

  const size = () => {
    const el = panelRef.current;
    return { w: el?.offsetWidth ?? PANEL_W, h: el?.offsetHeight ?? 400 };
  };
  const viewport = () => ({ w: window.innerWidth, h: window.innerHeight });

  useEffect(() => {
    const keepInside = () => setPos((p) => clampPanel(p, size(), viewport()));
    keepInside();
    window.addEventListener('resize', keepInside);
    return () => window.removeEventListener('resize', keepInside);
    // size() reads the ref at call time; the listener is set once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const onPointerDown = (e) => {
    if (e.button !== 0 || e.target.closest('button, select, input')) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { px: e.clientX, py: e.clientY, x: pos.x, y: pos.y };
  };
  const onPointerMove = (e) => {
    const d = drag.current;
    if (!d) return;
    setPos(clampPanel({ x: d.x + e.clientX - d.px, y: d.y + e.clientY - d.py }, size(), viewport()));
  };
  const onPointerUp = () => {
    if (!drag.current) return;
    drag.current = null;
    setPos((p) => {
      savePos(p);
      return p;
    });
  };
  const nudge = (dx, dy) => setPos((p) => {
    const next = clampPanel({ x: p.x + dx, y: p.y + dy }, size(), viewport());
    savePos(next);
    return next;
  });

  return { pos, handlers: { onPointerDown, onPointerMove, onPointerUp, onPointerCancel: onPointerUp }, nudge };
}

function SetupPanel({
  onClose, onRefresh, selectedDevices, onSelectedDevicesChange, availableDevices, audioStats, mixerController, isLive,
}) {
  const panelRef = useRef(null);
  const { pos, handlers, nudge } = useDragPanel(panelRef);
  const [draft, setDraft] = useState(() => draftFrom(selectedDevices));
  const [savedWhileLive, setSavedWhileLive] = useState(false);
  // Saved devices apply when the engine next starts, so once it stops the note no longer holds.
  const [prevLive, setPrevLive] = useState(isLive);
  if (isLive !== prevLive) {
    setPrevLive(isLive);
    if (!isLive) setSavedWhileLive(false);
  }
  // Only the desktop engine has real meters; the browser's demo mixer simulates levels, which
  // must never pass for a working device.
  const feed = useMeterFeed(isTauri ? mixerController : null, isTauri);
  const dirty = isDirty(selectedDevices, draft);

  const set = (key) => (value) => setDraft((d) => ({ ...d, [key]: value }));
  const status = (key) => rowStatus({ saved: selectedDevices, draft, key, running: isLive });
  const save = () => {
    onSelectedDevicesChange(draftChanges(selectedDevices, draft));
    setSavedWhileLive(isLive);
  };
  const revert = () => setDraft(draftFrom(selectedDevices));

  const onKeyDown = useCallback((e) => {
    if (e.key === 'Escape' && !dirty) onClose();
  }, [dirty, onClose]);
  useEffect(() => {
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [onKeyDown]);

  const onHeaderKey = (e) => {
    const step = e.shiftKey ? 40 : 10;
    const moves = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] };
    if (moves[e.key]) {
      e.preventDefault();
      nudge(...moves[e.key]);
    }
  };

  const cables = availableDevices.outputs.filter((device) =>
    VIRTUAL_KEYWORDS.some((keyword) => (device.label || '').toLowerCase().includes(keyword)));
  const lowRate = audioStats.sampleRate > 0 && audioStats.sampleRate < 44100;

  return (
    <section
      ref={panelRef}
      className="fp-setup"
      style={{ left: pos.x, top: pos.y }}
      role="dialog"
      aria-modal="false"
      aria-labelledby="setup-title"
    >
      <div className="fp-setup-hd" {...handlers}>
        <span
          id="setup-title"
          className="fp-setup-grip"
          tabIndex={0}
          role="button"
          aria-label="Setup panel: drag, or use the arrow keys, to move it"
          onKeyDown={onHeaderKey}
        >
          <span aria-hidden="true" className="fp-grip" />
          Setup
        </span>
        <span className="fp-keys">
          <Key onClick={onRefresh} aria-label="refresh device list">refresh</Key>
          {dirty
            ? <TwoPressKey label="close and discard changes" onConfirm={onClose}>close</TwoPressKey>
            : <Key onClick={onClose} aria-label="close setup">close</Key>}
        </span>
      </div>

      <div className="fp-setup-bd">
        <DeviceRow id="dev-input" label="Input" tip="The mic or mixer the engine listens to."
          status={status('inputId')} value={draft.inputId ?? 'default'} onChange={set('inputId')}
          getDb={() => inputPeakDb(feed.current)}>
          <option value="default">System default</option>
          {deviceOptions(availableDevices.inputs, 'Mic')}
        </DeviceRow>
        <ComputerAudioToggle />

        <DeviceRow id="dev-monitor" label="Monitor" tip="What you hear: headphones or speakers for the sound engineer."
          status={status('outputId')} value={draft.outputId ?? 'default'} onChange={set('outputId')}
          getDb={() => busPeakDb(feed.current, 'monitor')}>
          <option value="default">System default</option>
          {deviceOptions(availableDevices.outputs, 'Speaker')}
        </DeviceRow>

        <DeviceRow id="dev-broadcast" label="Broadcast"
          tip="What the stream hears. For OBS or another app on this computer, NDI in the output router needs no extra device."
          status={status('broadcastBus')} value={draft.broadcastBus ?? 'Not set'} onChange={set('broadcastBus')}
          getDb={() => busPeakDb(feed.current, 'stream')}>
          <option value="Not set">Not set</option>
          <option value="Same as monitor">Same as monitor</option>
          {deviceOptions(availableDevices.outputs, 'Bus')}
        </DeviceRow>
        {cables.length > 0 && <Lamp tone="g">virtual device: {cables.map((device) => device.label).join(', ')}</Lamp>}

        {isTauri && (
          <DeviceRow id="dev-main" label="Main" tip="The PA or room. Use a different device from monitor and broadcast."
            status={status('mainOutputId')} value={draft.mainOutputId || 'Not set'} onChange={set('mainOutputId')}
            getDb={() => busPeakDb(feed.current, 'main')}>
            <option value="Not set">Not set</option>
            {deviceOptions(availableDevices.outputs, 'Output')}
          </DeviceRow>
        )}

        <div className="fp-glass fp-setup-fmt">
          <Lamp tone={isLive ? 'g' : 'off'}>{isLive ? 'engine on' : 'engine off'}</Lamp>
          <span className={`fp-num ${lowRate ? 'fp-caution' : ''}`}>
            {audioStats.sampleRate > 0 ? `${(audioStats.sampleRate / 1000).toFixed(1)} kHz` : '-- kHz'}
          </span>
          <span className="fp-num">{audioStats.bufferSize || '--'} smp</span>
        </div>
      </div>

      <div className="fp-setup-ft">
        {dirty && <Lamp tone="a">unsaved</Lamp>}
        {!dirty && savedWhileLive && (
          <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
            <Lamp tone="a">applies on next start</Lamp>
            <InfoTip label="Applying devices">The engine opens devices when it starts. Stop and start it to use the new ones.</InfoTip>
          </span>
        )}
        <span className="fp-bar-end">
          <Key onClick={revert} disabled={!dirty}>revert</Key>
          <Key lit={dirty ? 'sel' : null} onClick={save} disabled={!dirty}>save</Key>
        </span>
      </div>
    </section>
  );
}

/**
 * The setup panel: a floating, draggable, non-modal plate, so the desk stays usable while it is
 * open. Device choices are a draft until "save"; each device shows its state and a live level.
 */
export default function DeviceSettingsModal(props) {
  if (!props.open) return null;
  return <SetupPanel {...props} />;
}

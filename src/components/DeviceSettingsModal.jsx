import { useCallback, useEffect, useRef, useState } from 'react';

import {
  clampPanel, draftChanges, draftFrom, isDirty, readPanelPos, rowStatus,
} from '../lib/setupDraft.js';
import { isTauri } from '../lib/platform';
import {
  PAIR_KEYS, clockLamp, countOptions, deviceOptionLabel, findDevice, firstChannelOptions, isMissing, linkFor,
  pairLabel, pairOptions, showPairPicker, withDeviceChange,
} from '../lib/usbDevices.js';
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
const STATUS_LAMP = {
  on: ['g', 'on'],
  idle: ['off', 'idle'],
  'not set': ['off', 'not set'],
  unsaved: ['a', 'unsaved'],
  waiting: ['a', 'waiting'],
  lost: ['r', 'lost'],
  failed: ['r', 'failed'],
  missing: ['r', 'missing'],
};

/** The listed devices, plus the chosen one kept as "· missing" while it is unplugged. */
const deviceOptions = (devices, value) => [
  ...(isMissing(devices, value) ? [<option key={`missing:${value}`} value={value}>{`${value} · missing`}</option>] : []),
  ...devices.map((device) => (
    <option key={device.deviceId} value={device.deviceId}>{deviceOptionLabel(device)}</option>
  )),
];

/** The device a row's value plays on: the chosen one, or the system default. */
const rowDevice = (devices, value) => (
  value === 'default' ? devices.find((d) => d.isDefault) ?? null : findDevice(devices, value)
);

/** `options`, plus the saved `value` (labelled `label`) when the device does not offer it. */
const withValue = (options, value, label) => (
  options.some((o) => o.value === value) ? options : [...options, { value, label }]
);

/** A small channel picker shown beside a device that has more than two channels. */
function ChannelSelect({ label, value, options, onChange, narrow = false }) {
  return (
    <select className={`fp-field fp-field-ch${narrow ? ' fp-field-narrow' : ''}`} aria-label={label} value={value ?? ''}
      onChange={(event) => onChange(event.target.value === '' ? null : Number(event.target.value))}>
      {options.map((o) => <option key={o.label} value={o.value ?? ''}>{o.label}</option>)}
    </select>
  );
}

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

/**
 * One device: label, ⓘ, status lamp, the select (with an optional channel picker beside it), and
 * a live level bar while it is on.
 */
function DeviceRow({ id, label, tip, status, value, onChange, getDb, picker, error, children }) {
  const [tone, word] = STATUS_LAMP[status] ?? STATUS_LAMP.idle;
  return (
    <div className="fp-setup-row">
      <div className="fp-row">
        <label className="fp-label" htmlFor={id} style={{ margin: 0 }}>
          {label}
          {tip && <InfoTip label={label}>{tip}</InfoTip>}
        </label>
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
          <Lamp tone={tone}>{word}</Lamp>
          {error && <InfoTip label={`${label} error`}>{error}</InfoTip>}
        </span>
      </div>
      <div className="fp-setup-pick">
        <select id={id} className="fp-field" value={value} onChange={(event) => onChange(event.target.value)}>
          {children}
        </select>
        {picker}
      </div>
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
  deviceLinks,
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
  // A device change also resets its pair or input range (usbDevices.withDeviceChange).
  const setDevice = (key) => (value) => setDraft((d) => withDeviceChange(d, key, value));
  const links = deviceLinks?.links ?? [];
  const { inputs, outputs } = availableDevices;
  const status = (key, role, devices) => rowStatus({
    saved: selectedDevices,
    draft,
    key,
    running: isLive,
    link: linkFor(links, role),
    missing: isMissing(devices, selectedDevices[key]),
  });
  const errorOf = (role) => (isLive ? linkFor(links, role)?.error ?? null : null);
  // The output pair picker, only for a device with more than two channels.
  const pairPicker = (bus, value, label) => {
    const device = rowDevice(outputs, value);
    const key = PAIR_KEYS[bus];
    const pair = draft[key] ?? 0;
    if (!isTauri || !showPairPicker(device, pair)) return null;
    // A saved pair the device does not have stays listed, so it can be seen and changed.
    const options = withValue(pairOptions(device.channels), pair, pairLabel(pair));
    return <ChannelSelect label={`${label} output pair`} value={pair} options={options} onChange={set(key)} />;
  };
  const inputDevice = rowDevice(inputs, draft.inputId ?? 'default');
  const inputPicker = isTauri && inputDevice && (inputDevice.channels > 2 || (draft.inputFirst ?? 0) > 0 || draft.inputCount) && (
    <>
      <span className="fp-kl" aria-hidden="true">ch</span>
      <ChannelSelect narrow label="First input channel" value={draft.inputFirst ?? 0}
        options={withValue(firstChannelOptions(inputDevice.channels), draft.inputFirst ?? 0, String((draft.inputFirst ?? 0) + 1))} onChange={(v) => setDraft((d) => ({ ...d, inputFirst: v, inputCount: null }))} />
      <ChannelSelect narrow label="Input channel count" value={draft.inputCount ?? null}
        options={countOptions(inputDevice.channels, draft.inputFirst ?? 0)} onChange={set('inputCount')} />
    </>
  );
  const clock = isLive ? clockLamp(links, deviceLinks?.slipping) : null;
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

  const cables = outputs.filter((device) =>
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
        <DeviceRow id="dev-input" label="Input"
          tip="The mic, mixer or USB interface the engine listens to. With more than two channels, pick the first channel and how many become strips."
          status={status('inputId', 'input', inputs)} error={errorOf('input')}
          value={draft.inputId ?? 'default'} onChange={setDevice('inputId')} picker={inputPicker}
          getDb={() => inputPeakDb(feed.current)}>
          <option value="default">System default</option>
          {deviceOptions(inputs, draft.inputId)}
        </DeviceRow>
        <ComputerAudioToggle />

        <DeviceRow id="dev-monitor" label="Monitor" tip="What you hear: headphones or speakers for the sound engineer."
          status={status('outputId', 'monitor', outputs)} error={errorOf('monitor')}
          value={draft.outputId ?? 'default'} onChange={setDevice('outputId')}
          picker={pairPicker('monitor', draft.outputId ?? 'default', 'Monitor')}
          getDb={() => busPeakDb(feed.current, 'monitor')}>
          <option value="default">System default</option>
          {deviceOptions(outputs, draft.outputId)}
        </DeviceRow>

        <DeviceRow id="dev-broadcast" label="Broadcast"
          tip="What the stream hears. For OBS or another app on this computer, NDI in the output router needs no extra device."
          status={status('broadcastBus', 'stream', outputs)} error={errorOf('stream')}
          value={draft.broadcastBus ?? 'Not set'} onChange={setDevice('broadcastBus')}
          picker={pairPicker('stream', draft.broadcastBus, 'Broadcast')}
          getDb={() => busPeakDb(feed.current, 'stream')}>
          <option value="Not set">Not set</option>
          <option value="Same as monitor">Same as monitor</option>
          {deviceOptions(outputs, draft.broadcastBus)}
        </DeviceRow>
        {cables.length > 0 && <Lamp tone="g">virtual device: {cables.map((device) => device.label).join(', ')}</Lamp>}

        {isTauri && (
          <DeviceRow id="dev-main" label="Main"
            tip="The PA or room. It can share a multi-output interface with another output on a different pair (main 1-2, stream 3-4)."
            status={status('mainOutputId', 'main', outputs)} error={errorOf('main')}
            value={draft.mainOutputId || 'Not set'} onChange={setDevice('mainOutputId')}
            picker={pairPicker('main', draft.mainOutputId, 'Main')}
            getDb={() => busPeakDb(feed.current, 'main')}>
            <option value="Not set">Not set</option>
            {deviceOptions(outputs, draft.mainOutputId)}
          </DeviceRow>
        )}

        {isTauri && [
          { key: 'monitor2OutputId', bus: 'monitor2', id: 'dev-mon2', label: 'Monitor 2' },
          { key: 'monitor3OutputId', bus: 'monitor3', id: 'dev-mon3', label: 'Monitor 3' },
        ].map((m) => (
          <DeviceRow key={m.key} id={m.id} label={m.label}
            tip={`A second person's own monitor mix (${m.label.toLowerCase().replace('monitor', 'mon')} on the mixer). Use a device, or a pair on an interface, no other output uses.`}
            status={status(m.key, m.bus, outputs)} error={errorOf(m.bus)}
            value={draft[m.key] || 'Not set'} onChange={setDevice(m.key)}
            picker={pairPicker(m.bus, draft[m.key], m.label)}
            getDb={() => busPeakDb(feed.current, m.bus)}>
            <option value="Not set">Not set</option>
            {deviceOptions(outputs, draft[m.key])}
          </DeviceRow>
        ))}

        <div className="fp-glass fp-setup-fmt">
          <Lamp tone={isLive ? 'g' : 'off'}>{isLive ? 'engine on' : 'engine off'}</Lamp>
          {clock && (
            <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
              <Lamp tone={clock.tone}>{clock.word}</Lamp>
              <InfoTip label="Clock">
                Shared: every output runs on the input device's clock. Synced: an output on its own
                clock is kept in step. Slips: one ran dry in the last 10 s; check its USB cable or hub.
              </InfoTip>
            </span>
          )}
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

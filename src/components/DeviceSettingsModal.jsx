import { useEffect } from 'react';

import InfoTip from './faceplate/InfoTip.jsx';
import Key from './faceplate/Key.jsx';
import Lamp from './faceplate/Lamp.jsx';
import MainOutputSelect from './MainOutputSelect';

const VIRTUAL_KEYWORDS = ['vb-cable', 'vb cable', 'blackhole', 'black hole', 'loopback', 'virtual'];

function virtualCables(outputs) {
  return outputs.filter((device) =>
    VIRTUAL_KEYWORDS.some((keyword) => (device.label || '').toLowerCase().includes(keyword)));
}

function DeviceSelect({ id, label, tip, value, onChange, children }) {
  return (
    <div>
      <label className="fp-label" htmlFor={id}>
        {label}
        {tip && <InfoTip label={label}>{tip}</InfoTip>}
      </label>
      <select id={id} className="fp-field" value={value} onChange={(event) => onChange(event.target.value)}>
        {children}
      </select>
    </div>
  );
}

const deviceOptions = (devices, prefix) => devices.map((device) => (
  <option key={device.deviceId} value={device.deviceId}>
    {device.label || `${prefix} ${device.deviceId.slice(0, 5)}`}
  </option>
));

/** The setup dialog: input, monitor, broadcast and main devices, plus the engine format. */
export default function DeviceSettingsModal({
  open,
  onClose,
  onRefresh,
  selectedDevices,
  onSelectedDevicesChange,
  availableDevices,
  audioStats,
}) {
  useEffect(() => {
    if (!open) return undefined;
    const onKey = (e) => { if (e.key === 'Escape') onClose(); };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  if (!open) return null;

  const cables = virtualCables(availableDevices.outputs);
  const lowRate = audioStats.sampleRate > 0 && audioStats.sampleRate < 44100;

  return (
    <>
      <div className="fp-scrim" onClick={onClose} aria-hidden="true" />
      <div className="fp-dialog-wrap">
        <div className="fp-dialog" role="dialog" aria-modal="true" aria-labelledby="dev-title">
          <div className="fp-dialog-hd">
            <span id="dev-title">Setup</span>
            <span className="fp-keys">
              <Key onClick={onRefresh}>refresh</Key>
              <Key onClick={onClose} aria-label="close setup">close</Key>
            </span>
          </div>
          <div className="fp-dialog-bd">
            <DeviceSelect id="dev-input" label="Input (mic, mixer)" value={selectedDevices.inputId}
              onChange={(inputId) => onSelectedDevicesChange({ inputId })}>
              <option value="default">System default</option>
              {deviceOptions(availableDevices.inputs, 'Mic')}
            </DeviceSelect>

            <DeviceSelect id="dev-monitor" label="Monitor (what you hear)"
              tip="Headphones or speakers for the sound engineer."
              value={selectedDevices.outputId} onChange={(outputId) => onSelectedDevicesChange({ outputId })}>
              <option value="default">System default</option>
              {deviceOptions(availableDevices.outputs, 'Speaker')}
            </DeviceSelect>

            <div>
              <DeviceSelect id="dev-broadcast" label="Broadcast (what the stream hears)"
                tip="For OBS or another app on this computer, NDI in the output router needs no extra software. A virtual audio device also works: pick it here and the same device in OBS."
                value={selectedDevices.broadcastBus} onChange={(broadcastBus) => onSelectedDevicesChange({ broadcastBus })}>
                <option value="Not set">Not set</option>
                <option value="Same as monitor">Same as monitor</option>
                {deviceOptions(availableDevices.outputs, 'Bus')}
              </DeviceSelect>
              {cables.length > 0 && (
                <div style={{ marginTop: 6 }}>
                  <Lamp tone="g">virtual device: {cables.map((device) => device.label).join(', ')}</Lamp>
                </div>
              )}
            </div>

            <MainOutputSelect
              selectedDevices={selectedDevices}
              outputs={availableDevices.outputs}
              onChange={(mainOutputId) => onSelectedDevicesChange({ mainOutputId })}
            />

            <div className="fp-glass" style={{ padding: 10, display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
              <div>
                <div className="fp-glass-dim" style={{ fontSize: 'var(--fp-text-xs)' }}>sample rate</div>
                <div className={`fp-num ${lowRate ? 'fp-caution' : ''}`} style={{ fontSize: 'var(--fp-text-xl)' }}>
                  {audioStats.sampleRate > 0 ? `${(audioStats.sampleRate / 1000).toFixed(1)} kHz` : '--'}
                </div>
                {lowRate && <div className="fp-caution fp-sub">below 44.1 kHz</div>}
              </div>
              <div>
                <div className="fp-glass-dim" style={{ fontSize: 'var(--fp-text-xs)' }}>buffer</div>
                <div className="fp-num" style={{ fontSize: 'var(--fp-text-xl)' }}>{audioStats.bufferSize || '--'} smp</div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}

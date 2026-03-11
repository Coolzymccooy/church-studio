import React from 'react';
import {
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  RefreshCw,
  Settings,
  X,
} from 'lucide-react';

function VirtualCableHint({ outputs }) {
  const vcKeywords = ['vb-cable', 'vb cable', 'blackhole', 'black hole', 'loopback', 'virtual'];
  const detected = outputs.filter((device) =>
    vcKeywords.some((keyword) => (device.label || '').toLowerCase().includes(keyword)),
  );
  const isWindows = navigator.userAgent.toLowerCase().includes('win');
  const isMac = navigator.userAgent.toLowerCase().includes('mac');

  if (detected.length > 0) {
    return (
      <div className="mb-2 flex items-center gap-2 px-2.5 py-1.5 rounded-lg border border-emerald-700/40 bg-emerald-900/10">
        <CheckCircle2 size={13} className="text-emerald-400 shrink-0" />
        <span className="text-[10px] text-emerald-400 font-semibold">
          Virtual cable detected: {detected.map((device) => device.label).join(', ')}
        </span>
      </div>
    );
  }

  return (
    <div
      className="mb-2 rounded-lg border border-amber-700/30 px-3 py-2.5 space-y-1.5"
      style={{ background: 'rgba(var(--accent-rgb),0.05)' }}
    >
      <div className="flex items-start gap-2">
        <AlertTriangle size={12} className="text-amber-500 shrink-0 mt-0.5" />
        <p className="text-[10px] text-amber-400 font-semibold">No virtual cable detected</p>
      </div>
      <p className="text-[9px] text-slate-500 leading-relaxed">
        A virtual cable routes TIWATON audio into OBS. Install one, then refresh devices.
      </p>
      <div className="flex gap-2">
        {(isWindows || (!isWindows && !isMac)) && (
          <a
            href="https://vb-audio.com/Cable/"
            target="_blank"
            rel="noreferrer"
            className="flex items-center gap-1 px-2 py-1 rounded-md text-[9px] font-bold text-black"
            style={{ background: 'var(--accent)' }}
          >
            <ArrowRight size={9} />
            VB-CABLE (Windows)
          </a>
        )}
        {(isMac || (!isWindows && !isMac)) && (
          <a
            href="https://existential.audio/blackhole/"
            target="_blank"
            rel="noreferrer"
            className="flex items-center gap-1 px-2 py-1 rounded-md text-[9px] font-bold text-black"
            style={{ background: 'var(--accent)' }}
          >
            <ArrowRight size={9} />
            BlackHole (Mac)
          </a>
        )}
      </div>
      <p className="text-[9px] text-slate-600">After installing, click the refresh button above.</p>
    </div>
  );
}

export default function DeviceSettingsModal({
  open,
  onClose,
  onRefresh,
  selectedDevices,
  onSelectedDevicesChange,
  availableDevices,
  audioStats,
}) {
  if (!open) {
    return null;
  }

  return (
    <div className="fixed inset-0 bg-black/70 z-50 flex items-center justify-center backdrop-blur-sm p-4">
      <div className="bg-slate-900 w-full max-w-lg rounded-2xl border border-slate-700 shadow-2xl p-6 space-y-6">
        <div className="flex justify-between items-center border-b border-slate-800 pb-4">
          <h2 className="text-xl font-bold flex items-center gap-2">
            <Settings size={20} className="text-amber-400" />
            Device Settings
          </h2>
          <div className="flex gap-2">
            <button
              onClick={onRefresh}
              className="p-1.5 rounded-full bg-slate-800 text-slate-400 hover:text-white transition-colors"
              title="Refresh Device List"
            >
              <RefreshCw size={18} />
            </button>
            <button onClick={onClose} className="text-slate-400 hover:text-white">
              <X size={24} />
            </button>
          </div>
        </div>

        <div>
          <label className="text-xs font-bold text-slate-500 uppercase mb-2 block">
            Input Source (Mic / Mixer)
          </label>
          <select
            className="w-full bg-slate-950 border border-slate-700 rounded-lg p-3"
            value={selectedDevices.inputId}
            onChange={(event) => onSelectedDevicesChange({ inputId: event.target.value })}
          >
            <option value="default">Default System Microphone</option>
            {availableDevices.inputs.map((device) => (
              <option key={device.deviceId} value={device.deviceId}>
                {device.label || `Mic ${device.deviceId.slice(0, 5)}...`}
              </option>
            ))}
          </select>
        </div>

        <div>
          <label className="text-xs font-bold text-slate-500 uppercase mb-2 block">
            Monitoring Output (what you hear)
          </label>
          <select
            className="w-full bg-slate-950 border border-slate-700 rounded-lg p-3"
            value={selectedDevices.outputId}
            onChange={(event) => onSelectedDevicesChange({ outputId: event.target.value })}
          >
            <option value="default">Default System Output</option>
            {availableDevices.outputs.map((device) => (
              <option key={device.deviceId} value={device.deviceId}>
                {device.label || `Speaker ${device.deviceId.slice(0, 5)}...`}
              </option>
            ))}
          </select>
          <p className="mt-1 text-[10px] text-slate-500">
            Choose AirPods, headphones, or speakers the sound engineer will monitor on.
          </p>
        </div>

        <div>
          <label className="text-xs font-bold text-slate-500 uppercase mb-2 block">
            Broadcast Output (what OBS / YouTube hears)
          </label>

          <VirtualCableHint outputs={availableDevices.outputs} />

          <select
            className="w-full bg-slate-950 border border-slate-700 rounded-lg p-3"
            value={selectedDevices.broadcastBus}
            onChange={(event) => onSelectedDevicesChange({ broadcastBus: event.target.value })}
          >
            <option value="Not set">Not set yet</option>
            <option value="Same as monitor">Same as monitor output</option>
            {availableDevices.outputs.map((device) => (
              <option key={device.deviceId} value={device.deviceId}>
                {device.label || `Bus ${device.deviceId.slice(0, 5)}...`}
              </option>
            ))}
          </select>
          <p className="mt-1.5 text-[10px] text-slate-600 leading-snug">
            Set this to VB-CABLE Input (Windows) or BlackHole 2ch (Mac). Then in OBS, choose the same device as your Audio Input Capture source.
          </p>
        </div>

        <div className="grid grid-cols-2 gap-4">
          <div
            className={`p-4 rounded-xl border ${
              audioStats.sampleRate > 0 && audioStats.sampleRate < 44100
                ? 'bg-amber-900/20 border-amber-600/50'
                : 'bg-slate-800 border-slate-700'
            }`}
          >
            <div className="text-xs font-bold mb-1">SAMPLE RATE</div>
            <div
              className={`text-lg font-mono flex items-center gap-2 ${
                audioStats.sampleRate > 0 && audioStats.sampleRate < 44100
                  ? 'text-amber-500'
                  : 'text-amber-400'
              }`}
            >
              {audioStats.sampleRate > 0 ? `${(audioStats.sampleRate / 1000).toFixed(1)} kHz` : '--'}
              {audioStats.sampleRate > 0 && audioStats.sampleRate < 44100 && (
                <AlertTriangle size={16} />
              )}
            </div>
          </div>
          <div className="bg-slate-800 p-4 rounded-xl border border-slate-700">
            <div className="text-xs font-bold mb-1">BUFFER SIZE</div>
            <div className="text-lg font-mono text-amber-400">
              {audioStats.bufferSize || '--'} smp
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

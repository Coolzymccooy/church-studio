import React from 'react';

import { isTauri } from '../lib/platform';

/**
 * Picks the device for the mixer's Main (PA / room) bus. Only the desktop
 * engine has buses, so the browser build shows nothing.
 */
export default function MainOutputSelect({ selectedDevices, outputs, onChange }) {
  if (!isTauri) return null;

  return (
    <div>
      <label className="text-xs font-bold text-slate-500 uppercase mb-2 block">
        Main Output (PA / room mix)
      </label>
      <select
        className="w-full bg-slate-950 border border-slate-700 rounded-lg p-3"
        value={selectedDevices.mainOutputId || 'Not set'}
        onChange={(event) => onChange(event.target.value)}
      >
        <option value="Not set">Not set (Main bus not sent anywhere)</option>
        {outputs.map((device) => (
          <option key={device.deviceId} value={device.deviceId}>
            {device.label || `Output ${device.deviceId.slice(0, 5)}...`}
          </option>
        ))}
      </select>
      <p className="mt-1 text-[10px] text-slate-500">
        Carries the mixer&apos;s Main bus. Use a different device from Monitor and Broadcast.
      </p>
    </div>
  );
}

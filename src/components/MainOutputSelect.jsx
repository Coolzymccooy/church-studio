import { isTauri } from '../lib/platform';
import InfoTip from './faceplate/InfoTip.jsx';

/**
 * Picks the device for the mixer's Main (PA / room) bus. Only the desktop engine has buses, so
 * the browser build shows nothing.
 */
export default function MainOutputSelect({ selectedDevices, outputs, onChange }) {
  if (!isTauri) return null;

  return (
    <div>
      <label className="fp-label" htmlFor="dev-main">
        Main (PA, room)
        <InfoTip label="Main output">Carries the Main bus. Use a different device from monitor and broadcast.</InfoTip>
      </label>
      <select id="dev-main" className="fp-field" value={selectedDevices.mainOutputId || 'Not set'}
        onChange={(event) => onChange(event.target.value)}>
        <option value="Not set">Not set</option>
        {outputs.map((device) => (
          <option key={device.deviceId} value={device.deviceId}>
            {device.label || `Output ${device.deviceId.slice(0, 5)}`}
          </option>
        ))}
      </select>
    </div>
  );
}

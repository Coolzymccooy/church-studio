/**
 * The native engine's monitor level. The Monitor bus is scaled by `monitor_gain_db`
 * (src-tauri/src/audio/input_proc.rs), so the desk's monitor key sets it: on = unity, off = muted.
 */
export const MONITOR_MUTE_DB = -120;
export const MONITOR_PREF_KEY = 'tiwaton:monitor';

export function monitorGainDb(on) {
  return on ? 0 : MONITOR_MUTE_DB;
}

export function setNativeMonitor(invoke, on) {
  return invoke('set_param', { key: 'monitor_gain_db', value: monitorGainDb(on) });
}

/** A stored monitor preference ("on" / "off"); anything else gives the fallback. */
export function readMonitorPref(raw, fallback = false) {
  if (raw === 'on') return true;
  if (raw === 'off') return false;
  return fallback;
}

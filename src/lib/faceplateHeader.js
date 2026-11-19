/**
 * faceplateHeader.js — what the desk header shows: engine state, the engine readout, and the
 * status lamps. Pure functions over the app's existing state, so the header is testable and the
 * words stay short (label plus state, per the design standard).
 */
import { formatElapsed } from './recorder.js';

const CPU_WARN_PCT = 70;

const fixed = (v, digits) => (Number.isFinite(v) ? v.toFixed(digits) : null);

export function engineState(isLive, isPlayingFile) {
  if (isLive) return { word: 'running', tone: 'g' };
  if (isPlayingFile) return { word: 'file playback', tone: 'g' };
  return { word: 'standby', tone: 'off' };
}

/** The glass readout: sample rate, buffer, latency, cpu, callback avg / peak, dropped samples. */
export function headerReadout(stats) {
  const sr = stats.sampleRate > 0 ? `${(stats.sampleRate / 1000).toFixed(1)}k` : '--';
  const buf = stats.bufferSize > 0 ? String(stats.bufferSize) : '--';
  const lat = fixed(stats.latencyMs, 1);
  const cpu = fixed(stats.cpuLoadPct, 0);
  const avg = fixed(stats.callbackAvgMs, 2);
  const peak = fixed(stats.callbackPeakMs, 2);
  const dropped = Number.isFinite(stats.droppedOutputSamples) ? stats.droppedOutputSamples : 0;
  return [
    { key: 'sr', value: sr, warn: false },
    { key: 'buf', value: buf, warn: false },
    { key: 'lat', value: lat === null ? '--' : `${lat} ms`, warn: false },
    { key: 'cpu', value: cpu === null ? '--' : `${cpu}%`, warn: Number.isFinite(stats.cpuLoadPct) && stats.cpuLoadPct > CPU_WARN_PCT },
    { key: 'cb', value: avg === null ? '--' : (peak === null ? avg : `${avg} / ${peak}`), warn: false },
    { key: 'drop', value: String(dropped), warn: dropped > 0 },
  ];
}

export function obsLamp(status) {
  if (!status?.connected) return { word: 'obs', tone: 'off' };
  const parts = [status.streaming && 'live', status.recording && 'rec'].filter(Boolean);
  return { word: parts.length ? `obs ${parts.join(' · ')}` : 'obs', tone: status.recording ? 'r' : 'g' };
}

export function recLamp(status) {
  if (status?.recording) {
    const dropped = status.droppedFrames > 0 ? ` · ${status.droppedFrames} dropped` : '';
    // Red stays on while recording, even when frames drop: losing the recording cue is worse.
    return { word: `rec ${formatElapsed(status.elapsedSeconds)}${dropped}`, tone: 'r' };
  }
  if (status?.lastError) return { word: 'rec failed', tone: 'r' };
  return { word: 'rec', tone: 'off' };
}

/** gateClosed is the existing visualizer gate flag. */
export function gateLamp(isRunning, gateClosed) {
  if (!isRunning) return { word: 'gate', tone: 'off' };
  return gateClosed ? { word: 'gate closed', tone: 'off' } : { word: 'gate open', tone: 'g' };
}

export function formatClock(date) {
  return [date.getHours(), date.getMinutes(), date.getSeconds()].map((n) => String(n).padStart(2, '0')).join(':');
}

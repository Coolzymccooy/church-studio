/**
 * recorder.js — UI-side wrapper for multitrack service recording (desktop
 * only; docs/specs/2026-10-01-multitrack-recording-design.md, decision 9).
 *
 * Tauri commands (src-tauri/src/recorder_commands.rs):
 *   recorder_status()                                → RecorderStatus
 *   recorder_get_config()                            → { folder, defaultFolder, includeMain, armedStrips }
 *   recorder_set_config({ folder, includeMain, armedStrips }) → same as get
 *   recorder_start({ title, utcOffsetMinutes })      → RecorderStatus
 *   recorder_stop()                                  → RecorderStatus (with lastSummary)
 *   recorder_add_marker({ label })                   → { timeSeconds, label, source }
 *   recorder_open_folder()                           → ()
 * Event `recorder-status` (~1 s while recording, and once at the end).
 *
 * `armedStrips: null` means every active strip is armed.
 */
import { subscribeEvent } from './mixerEngine.js';

export const RECORDER_STATUS_EVENT = 'recorder-status';

const STATES = new Set(['unavailable', 'idle', 'recording']);

export const EMPTY_RECORDER_STATUS = Object.freeze({
  state: 'unavailable',
  recording: false,
  engineRunning: false,
  elapsedSeconds: 0,
  folder: null,
  tracks: [],
  droppedFrames: 0,
  markers: 0,
  lastError: null,
  lastSummary: null,
});

function blankToNull(text) {
  if (typeof text !== 'string') return null;
  const trimmed = text.trim();
  return trimmed ? trimmed : null;
}

function localUtcOffsetMinutes() {
  return -new Date().getTimezoneOffset();
}

export function createRecorderController({ invoke, listen, utcOffsetMinutes = localUtcOffsetMinutes }) {
  return {
    getStatus() {
      return invoke('recorder_status');
    },
    getConfig() {
      return invoke('recorder_get_config');
    },
    setConfig({ folder, includeMain, armedStrips } = {}) {
      return invoke('recorder_set_config', {
        folder: blankToNull(folder),
        includeMain: Boolean(includeMain),
        armedStrips: Array.isArray(armedStrips) ? armedStrips : null,
      });
    },
    start({ title } = {}) {
      return invoke('recorder_start', {
        title: blankToNull(title),
        utcOffsetMinutes: utcOffsetMinutes(),
      });
    },
    stop() {
      return invoke('recorder_stop');
    },
    addMarker(label) {
      return invoke('recorder_add_marker', { label: String(label ?? '') });
    },
    openFolder() {
      return invoke('recorder_open_folder');
    },
    subscribeStatus(cb) {
      return subscribeEvent(listen, RECORDER_STATUS_EVENT, cb);
    },
  };
}

/**
 * Register the event listener first, then read the snapshot, so a change
 * between the two is never missed; a snapshot landing after an event is
 * older and ignored. Returns a dispose function.
 */
export function followRecorderStatus(controller, onStatus) {
  let disposed = false;
  let sawEvent = false;
  const sub = controller.subscribeStatus((next) => {
    if (disposed || !next) return;
    sawEvent = true;
    onStatus(normalizeRecorderStatus(next));
  });
  Promise.resolve(sub.ready)
    .catch(() => {})
    .then(() => (disposed ? null : controller.getStatus()))
    .then((next) => {
      if (!disposed && !sawEvent && next) onStatus(normalizeRecorderStatus(next));
    })
    .catch(() => {});
  return () => {
    disposed = true;
    sub.dispose();
  };
}

let desktopController = null;

/** Shared instance that loads the Tauri APIs lazily; only call it when `isTauri`. */
export function getDesktopRecorderController() {
  if (!desktopController) {
    const invoke = (command, payload) => (
      import('@tauri-apps/api/core').then(({ invoke: tauriInvoke }) => tauriInvoke(command, payload))
    );
    const listen = (event, cb) => (
      import('@tauri-apps/api/event').then(({ listen: tauriListen }) => tauriListen(event, cb))
    );
    desktopController = createRecorderController({ invoke, listen });
  }
  return desktopController;
}

function finiteOr(value, fallback) {
  return Number.isFinite(value) ? value : fallback;
}

/** Backend status (command result or event payload) → UI shape. */
export function normalizeRecorderStatus(raw) {
  if (!raw || typeof raw !== 'object') return EMPTY_RECORDER_STATUS;
  const state = STATES.has(raw.state) ? raw.state : 'unavailable';
  return {
    state,
    recording: state === 'recording',
    engineRunning: Boolean(raw.engineRunning),
    elapsedSeconds: Math.max(0, finiteOr(raw.elapsedSeconds, 0)),
    folder: typeof raw.folder === 'string' ? raw.folder : null,
    tracks: Array.isArray(raw.tracks) ? raw.tracks.filter((t) => typeof t === 'string') : [],
    droppedFrames: Math.max(0, finiteOr(raw.droppedFrames, 0)),
    markers: Math.max(0, finiteOr(raw.markers, 0)),
    lastError: typeof raw.lastError === 'string' ? raw.lastError : null,
    lastSummary: raw.lastSummary && typeof raw.lastSummary === 'object' ? raw.lastSummary : null,
  };
}

/** Backend config → UI shape; armed strips sorted, `null` = all. */
export function normalizeRecorderConfig(raw) {
  const value = raw && typeof raw === 'object' ? raw : {};
  const armed = Array.isArray(value.armedStrips)
    ? [...new Set(value.armedStrips.filter((i) => Number.isInteger(i) && i >= 0))].sort((a, b) => a - b)
    : null;
  return {
    folder: typeof value.folder === 'string' ? value.folder : null,
    defaultFolder: typeof value.defaultFolder === 'string' ? value.defaultFolder : null,
    includeMain: Boolean(value.includeMain),
    armedStrips: armed,
  };
}

/** Which of `stripIndices` are armed (`armedStrips` null = all). */
export function armedIndices(armedStrips, stripIndices) {
  if (!Array.isArray(armedStrips)) return [...stripIndices];
  return stripIndices.filter((index) => armedStrips.includes(index));
}

/**
 * Toggle strip `index`. Returns the new `armedStrips`: `null` when every
 * strip in `stripIndices` ends up armed, else the explicit list.
 */
export function toggleArmedStrip(armedStrips, index, stripIndices) {
  const current = armedIndices(armedStrips, stripIndices);
  const next = current.includes(index)
    ? current.filter((i) => i !== index)
    : [...current, index].sort((a, b) => a - b);
  return next.length === stripIndices.length ? null : next;
}

/** "H:MM:SS". */
export function formatElapsed(seconds) {
  const total = Math.max(0, Math.floor(finiteOr(seconds, 0)));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = total % 60;
  return `${hours}:${String(minutes).padStart(2, '0')}:${String(secs).padStart(2, '0')}`;
}

/** Status-bar pill text, or null when not recording. */
export function recorderPillText(status) {
  if (!status?.recording) return null;
  return `REC ${formatElapsed(status.elapsedSeconds)}`;
}

/** One-line health: dropped frames while recording, or the last error. */
export function recorderHealth(status) {
  if (status?.lastError) {
    return { level: 'error', text: `Recording stopped: ${status.lastError}` };
  }
  const dropped = status?.droppedFrames ?? 0;
  if (dropped > 0) {
    return { level: 'warn', text: `${dropped} dropped frame${dropped === 1 ? '' : 's'} (disk too slow?)` };
  }
  return { level: 'ok', text: 'No dropped frames' };
}

export function describeRecorderError(err) {
  if (typeof err === 'string') return err;
  if (err && typeof err.message === 'string') return err.message;
  return 'The recorder did not respond.';
}

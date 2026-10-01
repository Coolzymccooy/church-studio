/**
 * obsControl.js — UI-side wrapper for the OBS Studio Tauri commands
 * (src-tauri/src/obs/commands.rs), in the same shape as mixerEngine.js:
 *
 *   - createObsController({ invoke, listen }) — thin wrapper, exact command
 *     names and argument shapes.
 *   - getDesktopObsController() — shared instance that loads the Tauri APIs
 *     lazily; only call it when `isTauri`.
 *
 * Plus pure, unit-tested helpers used by the OBS panel and status pill.
 */
import { subscribeEvent } from './mixerEngine.js';

export const OBS_STATUS_EVENT = 'obs-status';
export const OBS_DEFAULT_HOST = '127.0.0.1';
export const OBS_DEFAULT_PORT = 4455;

export function createObsController({ invoke, listen }) {
  return {
    getConfig() {
      return invoke('obs_get_config');
    },
    /**
     * password: undefined/null keeps the stored one, '' clears it.
     * sceneLink: undefined/null keeps the stored scene link.
     */
    setConfig({ host, port, password, enabled, sceneLink }) {
      return invoke('obs_set_config', {
        host,
        port,
        password: password ?? null,
        enabled: Boolean(enabled),
        sceneLink: sceneLink ?? null,
      });
    },
    getStatus() {
      return invoke('obs_status');
    },
    setScene(name) {
      return invoke('obs_set_scene', { name });
    },
    // The live-output commands refuse to act unless confirm === true.
    startStream(confirm) {
      return invoke('obs_start_stream', { confirm: confirm === true });
    },
    stopStream(confirm) {
      return invoke('obs_stop_stream', { confirm: confirm === true });
    },
    startRecord(confirm) {
      return invoke('obs_start_record', { confirm: confirm === true });
    },
    stopRecord(confirm) {
      return invoke('obs_stop_record', { confirm: confirm === true });
    },
    subscribeStatus(cb) {
      return subscribeEvent(listen, OBS_STATUS_EVENT, cb);
    },
  };
}

/**
 * Follow OBS status: register the `obs-status` listener first, then read the
 * snapshot, so a change between the two can never be missed. A snapshot that
 * lands after an event is older than that event and is ignored. Returns a
 * dispose function.
 */
export function followObsStatus(controller, onStatus) {
  let disposed = false;
  let sawEvent = false;
  const sub = controller.subscribeStatus((next) => {
    if (disposed || !next) return;
    sawEvent = true;
    onStatus(next);
  });
  Promise.resolve(sub.ready)
    .catch(() => {})
    .then(() => (disposed ? null : controller.getStatus()))
    .then((next) => {
      if (!disposed && !sawEvent && next) onStatus(next);
    })
    .catch(() => {});
  return () => {
    disposed = true;
    sub.dispose();
  };
}

let desktopController = null;

export function getDesktopObsController() {
  if (!desktopController) {
    const invoke = (command, payload) => (
      import('@tauri-apps/api/core').then(({ invoke: tauriInvoke }) => tauriInvoke(command, payload))
    );
    const listen = (event, cb) => (
      import('@tauri-apps/api/event').then(({ listen: tauriListen }) => tauriListen(event, cb))
    );
    desktopController = createObsController({ invoke, listen });
  }
  return desktopController;
}

// ── Live-output actions ──────────────────────────────────────────────────────

/** Confirm-dialog content for each live-output action. */
export const OBS_OUTPUT_ACTIONS = {
  startStream: {
    title: 'Start streaming?',
    message: 'OBS will go live to your streaming service now.',
    confirmLabel: 'Go live',
    danger: false,
  },
  stopStream: {
    title: 'Stop the live stream?',
    message: 'Viewers will be cut off. This ends the broadcast in OBS.',
    confirmLabel: 'Stop stream',
    danger: true,
  },
  startRecord: {
    title: 'Start recording?',
    message: 'OBS will start recording to its output folder.',
    confirmLabel: 'Start recording',
    danger: false,
  },
  stopRecord: {
    title: 'Stop recording?',
    message: 'OBS will finish and save the recording file.',
    confirmLabel: 'Stop recording',
    danger: true,
  },
};

// ── Pure helpers ─────────────────────────────────────────────────────────────

export const EMPTY_OBS_STATUS = Object.freeze({
  enabled: false,
  connecting: false,
  connected: false,
  obsVersion: null,
  currentScene: null,
  scenes: [],
  streaming: false,
  recording: false,
  streamTimecode: null,
  recordTimecode: null,
  lastError: null,
});

/** 'connected' | 'connecting' | 'disconnected' — drives the status dot. */
export function connectionState(status) {
  if (status?.connected) return 'connected';
  if (status?.connecting || status?.enabled) return 'connecting';
  return 'disconnected';
}

/** Text for the top status-bar pill, or null when it should be hidden. */
export function obsPillText(status) {
  if (!status?.connected) return null;
  const parts = [];
  if (status.streaming) parts.push('LIVE');
  if (status.recording) parts.push('REC');
  return parts.length > 0 ? `OBS ● ${parts.join(' / ')}` : 'OBS ●';
}

/** Tauri rejects with the command's error string; normalise anything else. */
export function describeObsError(err) {
  if (typeof err === 'string' && err.trim()) return err;
  if (err && typeof err.message === 'string' && err.message.trim()) return err.message;
  return 'Something went wrong talking to OBS';
}

/** Port field text → integer 1..65535, or null. */
export function parsePort(text) {
  const trimmed = String(text ?? '').trim();
  if (!/^\d{1,5}$/.test(trimmed)) return null;
  const port = Number(trimmed);
  return port >= 1 && port <= 65535 ? port : null;
}

/**
 * Return a new mapping with `key` → `value`; an empty value removes the key.
 * Keys differing only by case/whitespace are replaced. Never mutates `map`.
 */
export function setLinkEntry(map, key, value) {
  const cleanKey = String(key ?? '').trim();
  if (!cleanKey) return { ...(map || {}) };
  const wanted = cleanKey.toLowerCase();
  // Drop any spelling of the same scene so lookups stay unambiguous.
  const next = Object.fromEntries(
    Object.entries(map || {}).filter(([k]) => k.trim().toLowerCase() !== wanted),
  );
  const cleanValue = String(value ?? '').trim();
  if (cleanValue) next[cleanKey] = cleanValue;
  return next;
}

/** Case-insensitive, trimmed lookup mirroring the Rust scene link. */
export function lookupLink(map, key) {
  const wanted = String(key ?? '').trim().toLowerCase();
  if (!wanted || !map) return '';
  const hit = Object.entries(map).find(([k]) => k.trim().toLowerCase() === wanted);
  return hit ? hit[1].trim() : '';
}

/** Rows for the scene-link table: one per source scene with its target. */
export function buildLinkRows(sourceScenes, map) {
  return (sourceScenes || []).map((scene) => ({ scene, target: lookupLink(map, scene) }));
}

export function normalizeSceneLink(link) {
  return {
    enabled: Boolean(link?.enabled),
    studioToObs: { ...(link?.studioToObs || {}) },
    obsToStudio: { ...(link?.obsToStudio || {}) },
  };
}

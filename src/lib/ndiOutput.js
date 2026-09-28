/**
 * ndiOutput.js — UI-side helpers for NDI® audio output (desktop app only).
 *
 * Tauri commands (see src-tauri/src/ndi_commands.rs):
 *   ndi_status()               → { available, loading, version, path, error }
 *                                 (loading: the runtime is still loading in the background)
 *   ndi_get_outputs()          → { stream, main, monitor, base_name }
 *   ndi_set_outputs({outputs}) → { outputs, running, restart_required, message, save_error }
 *   engine_status().ndi        → { available, sending: [names], dropped_samples }
 *
 * `sanitizeNdiBaseName` mirrors `sanitize_base_name` in src-tauri/src/ndi/mod.rs.
 */

export const NDI_TOOLS_URL = 'https://ndi.video/tools';
export const NDI_URL = 'https://ndi.video';
export const NDI_TRADEMARK = 'NDI® is a registered trademark of Vizrt NDI AB';

export const DEFAULT_NDI_BASE_NAME = 'TIWATON Studio';
export const MAX_NDI_BASE_NAME_CHARS = 60;

/** The buses that can be sent, in display order. */
export const NDI_BUSES = [
  { key: 'stream', label: 'Stream' },
  { key: 'main', label: 'Main' },
  { key: 'monitor', label: 'Monitor' },
];

// eslint-disable-next-line no-control-regex
const CONTROL_CHARS = /[\u0000-\u001f\u007f-\u009f]/g;
const FORBIDDEN_CHARS = /[\\/:*?"<>|()]/g;

/**
 * Trim, collapse whitespace, drop control and forbidden characters and cap
 * at 60 characters. An empty result falls back to the default name.
 */
export function sanitizeNdiBaseName(raw) {
  const text = typeof raw === 'string' ? raw : '';
  const cleaned = text
    .replace(/\s/g, ' ')
    .replace(CONTROL_CHARS, '')
    .replace(FORBIDDEN_CHARS, '')
    .replace(/ +/g, ' ')
    .trim();
  const capped = Array.from(cleaned).slice(0, MAX_NDI_BASE_NAME_CHARS).join('').trimEnd();
  return capped || DEFAULT_NDI_BASE_NAME;
}

/** "TIWATON Studio" + "Stream" → "TIWATON Studio (Stream)". */
export function ndiSourceName(baseName, busLabel) {
  return `${sanitizeNdiBaseName(baseName)} (${busLabel})`;
}

/** UI shape → the `ndi_set_outputs` invoke arguments. */
export function buildNdiOutputsArgs({ stream, main, monitor, baseName } = {}) {
  return {
    outputs: {
      stream: Boolean(stream),
      main: Boolean(main),
      monitor: Boolean(monitor),
      base_name: sanitizeNdiBaseName(baseName),
    },
  };
}

/** Backend `NdiOutputs` → UI shape. Tolerates missing fields. */
export function normalizeNdiOutputs(raw) {
  const value = raw && typeof raw === 'object' ? raw : {};
  return {
    stream: Boolean(value.stream),
    main: Boolean(value.main),
    monitor: Boolean(value.monitor),
    baseName: sanitizeNdiBaseName(value.base_name ?? value.baseName),
  };
}

/** One-line runtime status for the panel header. */
export function describeNdiStatus(status) {
  if (!status || status.loading) return { available: false, label: 'Checking NDI® runtime…' };
  if (status.available) {
    return {
      available: true,
      label: status.version ? `NDI® runtime ${status.version}` : 'NDI® runtime available',
    };
  }
  return { available: false, label: 'Not installed — get NDI Tools' };
}

/** Source names the running engine is sending (from `engine_status`). */
export function sendingFromEngineStatus(engineStatus) {
  const sending = engineStatus?.ndi?.sending;
  return Array.isArray(sending) ? sending.filter((name) => typeof name === 'string') : [];
}

/** Thin wrapper around the Tauri commands. */
export function createNdiClient(invoke) {
  return {
    getStatus: () => invoke('ndi_status'),
    getOutputs: async () => normalizeNdiOutputs(await invoke('ndi_get_outputs')),
    setOutputs: async (outputs) => {
      const result = await invoke('ndi_set_outputs', buildNdiOutputsArgs(outputs));
      return {
        outputs: normalizeNdiOutputs(result?.outputs),
        running: Boolean(result?.running),
        restartRequired: Boolean(result?.restart_required),
        message: typeof result?.message === 'string' ? result.message : null,
        saveError: typeof result?.save_error === 'string' ? result.save_error : null,
      };
    },
    getSending: async () => sendingFromEngineStatus(await invoke('engine_status')),
  };
}

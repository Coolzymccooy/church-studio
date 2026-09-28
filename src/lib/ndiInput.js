/**
 * ndiInput.js — UI-side helpers for NDI® audio input (desktop app only).
 *
 * Tauri commands (see src-tauri/src/ndi_input_commands.rs):
 *   ndi_list_sources()        → [{ name, url, own }]   (takes a second or two)
 *   ndi_get_inputs()          → { sources: [names] }
 *   ndi_set_inputs({inputs})  → { inputs, running, restart_required, message, save_error }
 *   engine_status().ndi.inputs → [{ source, strip, connected, fill_ms, dropped_samples, underruns }]
 *
 * Each selected source becomes a mixer strip after the hardware channels at
 * the next engine start. `sanitizeNdiSourceList` mirrors `sanitize_sources`
 * in src-tauri/src/ndi/receive/settings.rs.
 */

export const MAX_NDI_INPUTS = 4;
export const MAX_NDI_SOURCE_NAME_CHARS = 256;

// eslint-disable-next-line no-control-regex
const CONTROL_CHARS = /[\u0000-\u001f\u007f-\u009f]/;

/** Trim, drop invalid names and duplicates, keep at most four. */
export function sanitizeNdiSourceList(raw) {
  const list = Array.isArray(raw) ? raw : [];
  const clean = [];
  for (const entry of list) {
    if (clean.length === MAX_NDI_INPUTS) break;
    if (typeof entry !== 'string') continue;
    const name = entry.trim();
    const valid = name.length > 0
      && Array.from(name).length <= MAX_NDI_SOURCE_NAME_CHARS
      && !CONTROL_CHARS.test(name);
    if (valid && !clean.includes(name)) clean.push(name);
  }
  return clean;
}

/** Selection after (un)checking `name`; unchanged when already full. */
export function toggleNdiSource(selected, name, checked) {
  const current = sanitizeNdiSourceList(selected);
  if (!checked) return current.filter((entry) => entry !== name);
  if (current.includes(name) || current.length >= MAX_NDI_INPUTS) return current;
  return sanitizeNdiSourceList([...current, name]);
}

/** Source names → the `ndi_set_inputs` invoke arguments. */
export function buildNdiInputsArgs(sources = []) {
  return { inputs: { sources: sanitizeNdiSourceList(sources) } };
}

/** Backend `NdiInputs` → UI shape. */
export function normalizeNdiInputs(raw) {
  return { sources: sanitizeNdiSourceList(raw?.sources) };
}

/** `ndi_list_sources` result → [{ name, url, own }]; drops bad entries. */
export function normalizeNdiSources(raw) {
  if (!Array.isArray(raw)) return [];
  return raw
    .filter((entry) => entry && typeof entry.name === 'string' && entry.name.trim())
    .map((entry) => ({
      name: entry.name.trim(),
      url: typeof entry.url === 'string' ? entry.url : null,
      own: Boolean(entry.own),
    }));
}

/**
 * Found sources plus selected ones that were not found (shown as offline,
 * so they can still be unchecked), each with an `offline` flag.
 */
export function mergeSourceList(found, selected) {
  const rows = found.map((source) => ({ ...source, offline: false }));
  for (const name of sanitizeNdiSourceList(selected)) {
    if (!rows.some((row) => row.name === name)) {
      rows.push({ name, url: null, own: false, offline: true });
    }
  }
  return rows;
}

const count = (value) => (Number.isFinite(value) ? value : 0);

/** Live input status from `engine_status`. */
export function inputsFromEngineStatus(engineStatus) {
  const inputs = engineStatus?.ndi?.inputs;
  if (!Array.isArray(inputs)) return [];
  return inputs
    .filter((entry) => entry && typeof entry.source === 'string')
    .map((entry) => ({
      source: entry.source,
      strip: count(entry.strip),
      connected: Boolean(entry.connected),
      fillMs: count(entry.fill_ms),
      droppedSamples: count(entry.dropped_samples),
      underruns: count(entry.underruns),
    }));
}

/** Thin wrapper around the Tauri commands. */
export function createNdiInputClient(invoke) {
  return {
    listSources: async () => normalizeNdiSources(await invoke('ndi_list_sources')),
    getInputs: async () => normalizeNdiInputs(await invoke('ndi_get_inputs')),
    setInputs: async (sources) => {
      const result = await invoke('ndi_set_inputs', buildNdiInputsArgs(sources));
      return {
        inputs: normalizeNdiInputs(result?.inputs),
        running: Boolean(result?.running),
        restartRequired: Boolean(result?.restart_required),
        message: typeof result?.message === 'string' ? result.message : null,
        saveError: typeof result?.save_error === 'string' ? result.save_error : null,
      };
    },
    getInputStatus: async () => inputsFromEngineStatus(await invoke('engine_status')),
  };
}

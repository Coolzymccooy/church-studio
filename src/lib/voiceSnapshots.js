/**
 * Voice snapshots (Session → Snapshots): the whole voice setup saved under one key so it can be
 * recalled mid-service. Kept in localStorage so they survive closing the app and "reset".
 */
export const SNAPSHOTS_KEY = 'tiwaton:snapshots';

const SPEAKER_PRESETS = new Set(['balanced', 'pastor', 'guest', 'choir']);

function finiteOrNull(value) {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function cleanFeatures(features) {
  return Object.fromEntries(
    Object.entries(features).filter(([, on]) => typeof on === 'boolean'),
  );
}

function cleanSnapshot(raw) {
  if (!raw || typeof raw !== 'object') return null;
  if (!Number.isInteger(raw.id) || raw.id < 1) return null;
  if (!raw.features || typeof raw.features !== 'object' || Array.isArray(raw.features)) return null;
  return {
    id: raw.id,
    label: typeof raw.label === 'string' && raw.label ? raw.label : `Snapshot ${raw.id}`,
    features: cleanFeatures(raw.features),
    speakerPreset: SPEAKER_PRESETS.has(raw.speakerPreset) ? raw.speakerPreset : 'balanced',
    selectedNoiseProfileId: typeof raw.selectedNoiseProfileId === 'string' ? raw.selectedNoiseProfileId : '',
    noiseFloorThreshold: finiteOrNull(raw.noiseFloorThreshold),
    inputGainValue: finiteOrNull(raw.inputGainValue),
  };
}

/** Stored text → a clean list. Anything missing or malformed loads as no snapshots. */
export function parseSnapshots(text) {
  if (!text) return [];
  let data;
  try {
    data = JSON.parse(text);
  } catch {
    return [];
  }
  if (!Array.isArray(data)) return [];
  return data.map(cleanSnapshot).filter(Boolean);
}

export function serializeSnapshots(list) {
  return JSON.stringify(list);
}

/** "Snapshot n" keeps counting from the highest number saved. */
export function nextSnapshotId(list) {
  return list.reduce((max, s) => Math.max(max, s.id), 0) + 1;
}

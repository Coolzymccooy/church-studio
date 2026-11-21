import { INPUT_RANGE_KEYS, PAIR_KEYS } from './usbDevices.js';

/** Device keys the setup panel edits: devices, output pairs, the input channel range. */
export const SETUP_KEYS = [
  'inputId', 'outputId', 'broadcastBus', 'mainOutputId', 'monitor2OutputId', 'monitor3OutputId',
  ...Object.values(PAIR_KEYS), ...INPUT_RANGE_KEYS,
];

/** The keys whose draft value differs from the saved one, as a patch object. */
export function draftChanges(saved, draft, keys = SETUP_KEYS) {
  const patch = {};
  for (const key of keys) {
    if ((draft?.[key] ?? null) !== (saved?.[key] ?? null)) patch[key] = draft[key];
  }
  return patch;
}

export function isDirty(saved, draft, keys = SETUP_KEYS) {
  return Object.keys(draftChanges(saved, draft, keys)).length > 0;
}

/** A copy of the saved devices holding only the setup keys. */
export function draftFrom(saved, keys = SETUP_KEYS) {
  return Object.fromEntries(keys.map((key) => [key, saved?.[key] ?? null]));
}

const EDGE = 8;

/** Keeps a floating panel of `size` fully inside the `viewport` (at least EDGE px in). */
export function clampPanel(pos, size, viewport) {
  const maxX = Math.max(EDGE, viewport.w - size.w - EDGE);
  const maxY = Math.max(EDGE, viewport.h - size.h - EDGE);
  return {
    x: Math.min(maxX, Math.max(EDGE, Math.round(pos.x))),
    y: Math.min(maxY, Math.max(EDGE, Math.round(pos.y))),
  };
}

/** Parses a stored position; anything malformed gives null. */
export function readPanelPos(raw) {
  try {
    const v = JSON.parse(raw);
    return Number.isFinite(v?.x) && Number.isFinite(v?.y) ? { x: v.x, y: v.y } : null;
  } catch {
    return null;
  }
}

const LINK_STATUS = { ok: 'on', waiting: 'waiting', lost: 'lost', failed: 'failed' };

/**
 * Row status for one device: 'unsaved' when the draft differs, 'not set' when nothing is chosen;
 * while the engine runs, its link state ('on', 'waiting', 'lost', 'failed'); otherwise 'missing'
 * when the saved device is not connected, else 'idle'.
 */
export function rowStatus({ saved, draft, key, running, notSet = ['Not set'], link = null, missing = false }) {
  if ((draft?.[key] ?? null) !== (saved?.[key] ?? null)) return 'unsaved';
  const value = saved?.[key];
  if (value === null || value === undefined || value === '' || notSet.includes(value)) return 'not set';
  if (running) return LINK_STATUS[link?.state] ?? 'on';
  return missing ? 'missing' : 'idle';
}

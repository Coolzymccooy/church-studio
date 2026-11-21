/**
 * usbDevices.js — USB and multichannel interfaces in setup and on the desk
 * (docs/specs/2026-11-20-usb-interfaces.md): device option labels, output pairs, the input
 * channel range, missing devices, and the lamps for each engine stream's link and clock.
 * Pure functions; words stay short (label plus state).
 */

/** Saved per-bus output pair keys, by engine bus id. */
export const PAIR_KEYS = {
  main: 'mainPair',
  stream: 'streamPair',
  monitor: 'monitorPair',
  monitor2: 'monitor2Pair',
  monitor3: 'monitor3Pair',
};

/** Saved input channel range keys (0-based first channel, count or null for all). */
export const INPUT_RANGE_KEYS = ['inputFirst', 'inputCount'];

/** The pair key each output device key carries. */
const DEVICE_PAIR_KEY = {
  outputId: PAIR_KEYS.monitor,
  broadcastBus: PAIR_KEYS.stream,
  mainOutputId: PAIR_KEYS.main,
  monitor2OutputId: PAIR_KEYS.monitor2,
  monitor3OutputId: PAIR_KEYS.monitor3,
};

/**
 * A new draft with device `key` set to `value`. A pair or input range chosen for the old device
 * goes back to the default (1-2, every channel), so it can never be left where no picker shows.
 */
export function withDeviceChange(draft, key, value) {
  if (key === 'inputId') return { ...draft, inputId: value, inputFirst: 0, inputCount: null };
  const pairKey = DEVICE_PAIR_KEY[key];
  return pairKey ? { ...draft, [key]: value, [pairKey]: 0 } : { ...draft, [key]: value };
}

/** Show the pair picker for a device with more than two channels, or while a saved pair is not 1-2. */
export function showPairPicker(device, pair) {
  if (!device) return false;
  return device.channels > 2 || (Number(pair) || 0) > 0;
}

const PLACEHOLDERS = new Set(['default', 'Not set', 'Same as monitor']);
const MAX_PAIR = 31;
const SLIP_WINDOW_MS = 10_000;

const wholeAtLeast = (value, min) => {
  const n = Number(value);
  return Number.isInteger(n) && n >= min ? n : null;
};

export function pairLabel(pair) {
  return `${pair * 2 + 1}-${pair * 2 + 2}`;
}

/** "UMC404HD 192k · 4 ch · usb": channels only past two, the transport only when USB. */
export function deviceOptionLabel(device) {
  const parts = [device.label || device.deviceId];
  if (device.channels > 2) parts.push(`${device.channels} ch`);
  if (device.transport === 'usb') parts.push('usb');
  return parts.join(' · ');
}

/** Output pairs a device offers: 1-2, 3-4, … (a mono or stereo device has only 1-2). */
export function pairOptions(channels) {
  const pairs = Math.max(1, Math.floor((Number(channels) || 0) / 2));
  return Array.from({ length: pairs }, (_, value) => ({ value, label: pairLabel(value) }));
}

/** First-channel choices for an input of `channels` channels (labels are 1-based). */
export function firstChannelOptions(channels) {
  const count = Math.max(1, Number(channels) || 1);
  return Array.from({ length: count }, (_, value) => ({ value, label: String(value + 1) }));
}

/** How many channels from `first`: all, or 1 … the rest. */
export function countOptions(channels, first) {
  const room = Math.max(1, (Number(channels) || 1) - (Number(first) || 0));
  return [{ value: null, label: 'all' }, ...Array.from({ length: room }, (_, i) => ({ value: i + 1, label: String(i + 1) }))];
}

function matches(device, id) {
  if (id === device.deviceId || id === device.label) return true;
  const legacy = id.includes(':') ? id.split(':').slice(1).join(':') : null;
  return legacy === device.label;
}

/** The listed device a saved id refers to (by name, or a legacy "index:name" id). */
export function findDevice(devices, id) {
  if (!id || PLACEHOLDERS.has(id)) return null;
  return devices.find((device) => matches(device, id)) ?? null;
}

/** A saved device choice that is not connected now. Placeholders are never missing. */
export function isMissing(devices, id) {
  if (!id || PLACEHOLDERS.has(id)) return false;
  return !devices.some((device) => matches(device, id));
}

/** `outputPairs` for `start_audio_engine`. */
export function buildOutputPairs(selected = {}) {
  return Object.fromEntries(Object.entries(PAIR_KEYS).map(([bus, key]) => {
    const pair = wholeAtLeast(selected[key], 0);
    return [bus, pair !== null && pair <= MAX_PAIR ? pair : 0];
  }));
}

/** `inputChannels` for `start_audio_engine`. */
export function buildInputChannels(selected = {}) {
  return {
    first: wholeAtLeast(selected.inputFirst, 0) ?? 0,
    count: wholeAtLeast(selected.inputCount, 1),
  };
}

/** The engine-status entry for one role (`input`, `main`, `stream`, `monitor`, …). */
export function linkFor(devices, role) {
  return (devices ?? []).find((d) => d.role === role) ?? null;
}

const LINK_LAMPS = {
  ok: { tone: 'g', word: 'on' },
  waiting: { tone: 'a', word: 'waiting' },
  lost: { tone: 'r', word: 'lost' },
  failed: { tone: 'r', word: 'failed' },
};

export function linkLamp(link) {
  return link ? LINK_LAMPS[link.state] ?? null : null;
}

/** The desk-header device lamp: the worst problem, or null when every device is fine. */
export function deviceAttention(devices, slipping) {
  const list = devices ?? [];
  for (const state of ['lost', 'failed', 'waiting']) {
    const hit = list.find((d) => d.state === state);
    if (hit) return { tone: state === 'waiting' ? 'a' : 'r', word: `${hit.name} ${state}` };
  }
  return slipping ? { tone: 'a', word: 'clock slips' } : null;
}

const totalUnderruns = (devices) => (devices ?? [])
  .filter((d) => d.clock === 'corrected')
  .reduce((sum, d) => sum + (Number(d.underruns) || 0), 0);

/**
 * Keep the last 10 s of underrun totals (a new array) and say whether they rose in that time.
 * A total that went down (an engine restart) starts the history again.
 */
export function trackUnderruns(history, devices, now) {
  const total = totalUnderruns(devices);
  // While a device is down every corrected output runs dry once: that is not a clock slip.
  if ((devices ?? []).some((d) => d.state === 'lost')) {
    return { history: [], slipping: false };
  }
  const last = history[history.length - 1];
  const kept = last && total < last.total ? [] : history.filter((h) => now - h.t <= SLIP_WINDOW_MS);
  const next = [...kept, { t: now, total }];
  return { history: next, slipping: next.length > 1 && total > next[0].total };
}

/** The setup clock lamp, or null while the engine is off. */
export function clockLamp(devices, slipping) {
  const outputs = (devices ?? []).filter((d) => d.role !== 'input');
  if (outputs.length === 0) return null;
  if (slipping) return { tone: 'a', word: 'clock slips' };
  return outputs.some((d) => d.clock === 'corrected')
    ? { tone: 'g', word: 'clock synced' }
    : { tone: 'g', word: 'clock shared' };
}

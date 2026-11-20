/**
 * netAudio.js — UI-side helpers for network audio (desktop app only).
 * Spec: docs/specs/2026-11-20-network-audio-design.md.
 *
 * Tauri commands (see src-tauri/src/net_audio_commands.rs):
 *   net_audio_get()             → config
 *   net_audio_set({ config })   → { config, running, restart_required, problem, save_error }
 *   net_audio_new_passphrase()  → string
 *   net_audio_local_addresses() → [ip]
 *   engine_status().net_audio   → { receiving, port, error, badPackets, peers, sending }
 *
 * Config: { receive: { enabled, port, slots, bufferMs },
 *           send: { enabled, target, bus, stereo, name }, passphrase }.
 * `sanitizeNetAudio` mirrors `NetAudioConfig::sanitized` in
 * src-tauri/src/netaudio/settings.rs.
 */

export const DEFAULT_PORT = 47810;
export const MAX_NET_INPUTS = 4;
export const DEFAULT_SLOTS = 2;
export const MIN_BUFFER_MS = 15;
export const MAX_BUFFER_MS = 200;
export const DEFAULT_BUFFER_MS = 20;
export const MIN_PASSPHRASE_CHARS = 8;
export const MAX_PASSPHRASE_CHARS = 128;
export const MAX_TARGET_CHARS = 255;
export const MAX_NAME_CHARS = 32;
export const BUSES = ['main', 'stream', 'monitor'];

// eslint-disable-next-line no-control-regex
const CONTROL_CHARS = /[\u0000-\u001f\u007f-\u009f]/g;

const int = (value, fallback) => (Number.isFinite(Number(value)) ? Math.trunc(Number(value)) : fallback);
const clamp = (value, lo, hi) => Math.min(hi, Math.max(lo, value));
const num = (value) => (Number.isFinite(value) ? value : 0);

function cleanText(text, max) {
  if (typeof text !== 'string') return '';
  return Array.from(text.replace(CONTROL_CHARS, '')).slice(0, max).join('').trim();
}

function sanitizePort(value) {
  const port = int(value, DEFAULT_PORT);
  return port >= 1 && port <= 65535 ? port : DEFAULT_PORT;
}

/** Every field in range, defaults for anything missing. */
export function sanitizeNetAudio(raw) {
  const receive = raw?.receive ?? {};
  const send = raw?.send ?? {};
  const bus = typeof send.bus === 'string' ? send.bus.trim().toLowerCase() : '';
  return {
    receive: {
      enabled: Boolean(receive.enabled),
      port: sanitizePort(receive.port),
      slots: clamp(int(receive.slots, DEFAULT_SLOTS), 1, MAX_NET_INPUTS),
      bufferMs: clamp(int(receive.bufferMs, DEFAULT_BUFFER_MS), MIN_BUFFER_MS, MAX_BUFFER_MS),
    },
    send: {
      enabled: Boolean(send.enabled),
      target: cleanText(send.target, MAX_TARGET_CHARS),
      bus: BUSES.includes(bus) ? bus : 'main',
      stereo: Boolean(send.stereo),
      name: cleanText(send.name, MAX_NAME_CHARS),
    },
    passphrase: cleanText(raw?.passphrase, MAX_PASSPHRASE_CHARS),
  };
}

/** Why network audio will not start, when receive or send is on. */
export function passphraseProblem(config) {
  const on = config?.receive?.enabled || config?.send?.enabled;
  if (!on) return null;
  const length = Array.from(config?.passphrase ?? '').length;
  return length < MIN_PASSPHRASE_CHARS ? `Passphrase needs ${MIN_PASSPHRASE_CHARS}+ characters` : null;
}

function normalizePeer(entry) {
  return {
    strip: num(entry.strip),
    label: typeof entry.label === 'string' ? entry.label : '',
    connected: Boolean(entry.connected),
    name: typeof entry.name === 'string' ? entry.name : '',
    address: typeof entry.address === 'string' ? entry.address : '',
    bufferMs: num(entry.bufferMs),
    jitterMs: num(entry.jitterMs),
    rttMs: num(entry.rttMs),
    latencyMs: num(entry.latencyMs),
    lostPackets: num(entry.lostPackets),
    underruns: num(entry.underruns),
    senderRate: num(entry.senderRate),
    rateMismatchPackets: num(entry.rateMismatchPackets),
  };
}

function normalizeSending(raw) {
  if (!raw || typeof raw !== 'object') return null;
  return {
    connected: Boolean(raw.connected),
    refused: raw.refused === 'full' || raw.refused === 'rate' ? raw.refused : null,
    address: typeof raw.address === 'string' ? raw.address : '',
    rttMs: num(raw.rttMs),
    packetsSent: num(raw.packetsSent),
    sendErrors: num(raw.sendErrors),
    droppedSamples: num(raw.droppedSamples),
  };
}

/** Live network-audio status from `engine_status`. */
export function netAudioFromEngineStatus(engineStatus) {
  const raw = engineStatus?.net_audio;
  const peers = Array.isArray(raw?.peers)
    ? raw.peers.filter((entry) => entry && typeof entry === 'object').map(normalizePeer)
    : [];
  return {
    receiving: Boolean(raw?.receiving),
    port: Number.isFinite(raw?.port) ? raw.port : null,
    error: typeof raw?.error === 'string' ? raw.error : null,
    badPackets: num(raw?.badPackets),
    peers,
    sending: normalizeSending(raw?.sending),
  };
}

/** Thin wrapper around the Tauri commands. */
export function createNetAudioClient(invoke) {
  return {
    get: async () => sanitizeNetAudio(await invoke('net_audio_get')),
    set: async (config) => {
      const result = await invoke('net_audio_set', { config: sanitizeNetAudio(config) });
      return {
        config: sanitizeNetAudio(result?.config),
        running: Boolean(result?.running),
        restartRequired: Boolean(result?.restart_required),
        problem: typeof result?.problem === 'string' ? result.problem : null,
        saveError: typeof result?.save_error === 'string' ? result.save_error : null,
      };
    },
    newPassphrase: async () => {
      const value = await invoke('net_audio_new_passphrase');
      return typeof value === 'string' ? value : '';
    },
    localAddresses: async () => {
      const list = await invoke('net_audio_local_addresses');
      return Array.isArray(list) ? list.filter((ip) => typeof ip === 'string') : [];
    },
    getStatus: async () => netAudioFromEngineStatus(await invoke('engine_status')),
  };
}

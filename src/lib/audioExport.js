/**
 * audioExport.js — MP3 / M4A (AAC) export on the desktop app (issue #29).
 *
 * Tauri commands (src-tauri/src/export_commands.rs):
 *   recorder_export({ track, format, bitrateKbps })  → ExportResult
 *     `track` is a track's first file in the last recording; the export is
 *     written next to it with the same base name.
 *   editor_export(<WAV bytes>, headers x-export-format, x-export-dest) → ExportResult
 * ExportResult: { path, file, format, durationSeconds, bytes }.
 * Event `export-progress`: { file, format, fraction } in 1% steps.
 * One export runs at a time; a second is refused.
 */
import { subscribeEvent } from './mixerEngine.js';

export const EXPORT_PROGRESS_EVENT = 'export-progress';
export const EXPORT_FORMATS = Object.freeze(['mp3', 'm4a']);
export const STREAM_MIX_NAME = 'Stream Mix';

export function createExportController({ invoke, listen }) {
  return {
    exportTrack({ track, format, bitrateKbps = null }) {
      return invoke('recorder_export', { track, format, bitrateKbps });
    },
    exportEditorAudio({ wavBytes, format, dest }) {
      return invoke('editor_export', wavBytes, {
        headers: {
          'x-export-format': format,
          'x-export-dest': encodeURIComponent(dest),
        },
      });
    },
    subscribeProgress(cb) {
      return subscribeEvent(listen, EXPORT_PROGRESS_EVENT, cb);
    },
  };
}

let desktopController = null;

/** Shared instance that loads the Tauri APIs lazily; only call it when `isTauri`. */
export function getDesktopExportController() {
  if (!desktopController) {
    const invoke = (command, payload, options) => (
      import('@tauri-apps/api/core').then(({ invoke: tauriInvoke }) => tauriInvoke(command, payload, options))
    );
    const listen = (event, cb) => (
      import('@tauri-apps/api/event').then(({ listen: tauriListen }) => tauriListen(event, cb))
    );
    desktopController = createExportController({ invoke, listen });
  }
  return desktopController;
}

/** The last recording's tracks as `{ name, file, kind }`, junk dropped. */
export function exportTracks(summary) {
  const tracks = Array.isArray(summary?.tracks) ? summary.tracks : [];
  return tracks
    .filter((t) => t && typeof t.name === 'string' && typeof t.file === 'string' && t.file)
    .map((t) => ({ name: t.name, file: t.file, kind: t.kind === 'bus' ? 'bus' : 'strip' }));
}

/** The Stream mix if recorded, else the first bus, else the first track. */
export function defaultExportTrack(tracks) {
  const list = Array.isArray(tracks) ? tracks : [];
  const pick = list.find((t) => t.kind === 'bus' && t.name === STREAM_MIX_NAME)
    ?? list.find((t) => t.kind === 'bus')
    ?? list[0];
  return pick ? pick.file : null;
}

/** Backend progress → `{ file, format, fraction }` with fraction in 0..1. */
export function normalizeExportProgress(raw) {
  const value = raw && typeof raw === 'object' ? raw : {};
  const fraction = Number.isFinite(value.fraction) ? Math.min(1, Math.max(0, value.fraction)) : 0;
  return {
    file: typeof value.file === 'string' ? value.file : '',
    format: EXPORT_FORMATS.includes(value.format) ? value.format : null,
    fraction,
  };
}

/** "42%". */
export function formatExportPercent(fraction) {
  const value = Number.isFinite(fraction) ? Math.min(1, Math.max(0, fraction)) : 0;
  return `${Math.floor(value * 100)}%`;
}

export function describeExportError(err) {
  if (typeof err === 'string') return err;
  if (err && typeof err.message === 'string') return err.message;
  return 'The export did not finish.';
}

/** Default save name for an editor export. */
export function editorExportName(format, now = Date.now()) {
  return `TIWATON_edited_${now}.${format}`;
}

/** Save-dialog filter for one format. */
export function exportFilter(format) {
  return format === 'm4a'
    ? { name: 'AAC audio (M4A)', extensions: ['m4a'] }
    : { name: 'MP3 audio', extensions: ['mp3'] };
}

/**
 * 16-bit PCM WAV from per-channel Float32Arrays (samples clamped to ±1).
 * Returns an ArrayBuffer.
 */
export function encodeWavPcm16(channels, sampleRate) {
  const nch = channels.length;
  const length = nch ? channels[0].length : 0;
  const blockAlign = nch * 2;
  const dataSize = length * blockAlign;
  const buffer = new ArrayBuffer(44 + dataSize);
  const view = new DataView(buffer);
  const writeStr = (offset, text) => {
    for (let i = 0; i < text.length; i++) view.setUint8(offset + i, text.charCodeAt(i));
  };
  writeStr(0, 'RIFF');
  view.setUint32(4, 36 + dataSize, true);
  writeStr(8, 'WAVE');
  writeStr(12, 'fmt ');
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, nch, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * blockAlign, true);
  view.setUint16(32, blockAlign, true);
  view.setUint16(34, 16, true);
  writeStr(36, 'data');
  view.setUint32(40, dataSize, true);
  let offset = 44;
  for (let i = 0; i < length; i++) {
    for (let ch = 0; ch < nch; ch++) {
      const s = Math.max(-1, Math.min(1, channels[ch][i]));
      view.setInt16(offset, s < 0 ? s * 0x8000 : s * 0x7FFF, true);
      offset += 2;
    }
  }
  return buffer;
}

/**
 * "Play" for the last recording (desktop): the engine plays a track of the
 * last recording into the Monitor output. Commands (src-tauri/src/preview_commands.rs):
 *   recorder_play({ track })  → PreviewStatus   (track null = the Stream mix)
 *   recorder_play_stop()      → PreviewStatus
 *   recorder_play_status()    → PreviewStatus
 * PreviewStatus = { playing, positionSeconds, durationSeconds }.
 */
const IDLE = { playing: false, positionSeconds: 0, durationSeconds: 0 };

export function createPreviewController({ invoke }) {
  return {
    play(track = null) {
      return invoke('recorder_play', { track: track || null });
    },
    stop() {
      return invoke('recorder_play_stop');
    },
    status() {
      return invoke('recorder_play_status');
    },
  };
}

const seconds = (v) => (typeof v === 'number' && Number.isFinite(v) && v >= 0 ? v : 0);

export function normalizePreviewStatus(raw) {
  if (!raw || typeof raw !== 'object') return { ...IDLE };
  return {
    playing: raw.playing === true,
    positionSeconds: seconds(raw.positionSeconds),
    durationSeconds: seconds(raw.durationSeconds),
  };
}

function clock(total) {
  const whole = Math.floor(total);
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`;
}

export function previewKeyLabel(status) {
  return status?.playing ? `stop ${clock(status.positionSeconds)}` : 'play';
}

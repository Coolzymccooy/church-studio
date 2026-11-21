/** Pure labels for the desktop remote mixer panel. */

/** A pairing code lives this long (the listener's code TTL). */
export const PAIR_SECONDS = 120;
/** A device seen this recently is still connected (its event stream re-checks every 500 ms). */
const LIVE_SECONDS = 5;

export function seenLabel(seconds) {
  if (!Number.isFinite(seconds)) return '';
  if (seconds < LIVE_SECONDS) return 'live';
  if (seconds < 60) return `${Math.floor(seconds)} s ago`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min ago`;
  return `${Math.floor(seconds / 3600)} h ago`;
}

export function countdownLabel(seconds) {
  const left = Math.max(0, Math.floor(seconds ?? 0));
  return `${Math.floor(left / 60)}:${String(left % 60).padStart(2, '0')}`;
}

export function addressLabel(url) {
  return (url ?? '').replace(/^https?:\/\//, '').replace(/\/$/, '');
}

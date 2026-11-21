export function changeBody(layout, target, kind, value) {
  return {
    layout,
    target,
    kind,
    ...(kind === 'mute' ? { muted: Boolean(value) } : { db: Number(value) }),
  };
}

export function dbLabel(db) {
  return db <= -90 ? 'Off' : `${db.toFixed(1)} dB`;
}

export function pairUrl(address, code) {
  const url = new URL('/', address);
  url.hash = new URLSearchParams({ code }).toString();
  return url.href;
}

export function pairCodeFromHash(hash) {
  const code = new URLSearchParams(hash.slice(1)).get('code');
  return /^\d{6}$/.test(code ?? '') ? code : '';
}

export const CONTROL_RETRY_WINDOW_MS = 2_000;
const RETRY_DELAY_MS = 40;

export async function drainControlQueue({ queue, active, post, respond, wait, now, isCurrent = () => true }) {
  while (queue.size) {
    const [id, entry] = queue.entries().next().value;
    queue.delete(id);
    if (!isCurrent(entry)) continue;
    if (entry.lastStatus !== undefined && now() >= entry.retryUntil) {
      respond(entry.lastStatus);
      continue;
    }
    active.add(id);
    try {
      const status = await post(entry.change);
      if (!isCurrent(entry)) continue;
      if ((status === 429 || status === 503) && now() + RETRY_DELAY_MS < entry.retryUntil) {
        if (!queue.has(id)) queue.set(id, { ...entry, lastStatus: status });
        await wait(RETRY_DELAY_MS);
      } else {
        active.delete(id);
        respond(status);
      }
    } finally {
      active.delete(id);
    }
  }
}

/**
 * After a fader is let go: the desk value to show now, or null to wait. Snapshots that arrive
 * while a fader is held (or its write is in flight) are kept but not shown, and the server only
 * sends a snapshot when the state changes, so without this the fader could stay on a stale level.
 */
export function settledLevel({ held, busy, shownDb, latestDb }) {
  if (held || busy) return null;
  if (typeof latestDb !== 'number' || !Number.isFinite(latestDb)) return null;
  return latestDb === shownDb ? null : latestDb;
}

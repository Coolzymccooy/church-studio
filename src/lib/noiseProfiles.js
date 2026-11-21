/**
 * Captured room noise profiles. Ids must never be reused: voice snapshots and the saved
 * selection refer to a profile by id, so a reused id would silently recall a different room.
 */
export function newNoiseProfileId(now = Date.now(), random = Math.random) {
  const stamp = Math.floor(now).toString(36);
  const salt = Math.floor(random() * 36 ** 6).toString(36).padStart(6, '0');
  return `np-${stamp}-${salt}`;
}

/** "Noise Profile n" keeps counting from the highest number among the saved profiles. */
export function nextNoiseProfileNumber(profiles) {
  if (!Array.isArray(profiles)) return 1;
  const highest = profiles.reduce((max, p) => {
    const m = /^Noise Profile (\d+)$/.exec(p?.label ?? '');
    return m ? Math.max(max, Number(m[1])) : max;
  }, 0);
  return highest + 1;
}

/** Desk finishes. graphite is the default dark chassis; bone is light; auto follows the system. */
export const FINISHES = ['graphite', 'bone', 'auto'];

/** A stored value, or graphite when it is missing or not a known finish. */
export function readFinish(value) {
  return FINISHES.includes(value) ? value : 'graphite';
}

/** The next finish in the cycle graphite → bone → auto → graphite. */
export function nextFinish(current) {
  const i = FINISHES.indexOf(readFinish(current));
  return FINISHES[(i + 1) % FINISHES.length];
}

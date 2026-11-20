// A believable, never-clipping level for the website's demo desk meters. Pure, so it is tested.

export const DEMO_FLOOR_DB = -48;
export const DEMO_CEIL_DB = -6;

const MID_DB = -20;
const SPAN_DB = 12;

/** Level in dBFS at time `t` (seconds) for meter `seed`: slow phrasing plus a faster syllable wobble. */
export function demoLevelDb(t, seed = 0) {
  const phrase = Math.sin(t * (0.55 + seed * 0.13) + seed * 1.7);
  const syllable = Math.sin(t * (5.1 + seed * 0.9) + seed * 2.3);
  const breath = Math.sin(t * 0.21 + seed) > 0.85 ? -18 : 0;
  const db = MID_DB + phrase * SPAN_DB * 0.6 + syllable * SPAN_DB * 0.4 + breath;
  return Math.min(DEMO_CEIL_DB, Math.max(DEMO_FLOOR_DB, db));
}

/** Widest an info tip box may be, in CSS px; matches .fp-tipbox max-width. */
export const TIP_MAX_W = 240;
const EDGE = 8;
const GAP = 6;

/**
 * Where to place a tip box (position: fixed) under the ⓘ key whose rect is `rect`, centred on it
 * but kept `EDGE` px inside the viewport, so no panel edge or scroll area can clip it.
 */
export function tipPosition(rect, viewportWidth, boxWidth = TIP_MAX_W) {
  const centre = rect.left + rect.width / 2;
  const maxLeft = Math.max(EDGE, viewportWidth - boxWidth - EDGE);
  const left = Math.min(maxLeft, Math.max(EDGE, centre - boxWidth / 2));
  return { top: rect.bottom + GAP, left };
}

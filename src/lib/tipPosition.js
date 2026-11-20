/** Widest an info tip box may be, in CSS px; matches .fp-tipbox max-width. */
export const TIP_MAX_W = 240;
const EDGE = 8;
const GAP = 6;

/**
 * Where to place a tip box (position: fixed) beside the ⓘ or key whose rect is `rect`: centred on
 * it, kept `EDGE` px inside the viewport, below it when there is room and above it otherwise, so no
 * panel edge, scroll area or the bottom of the window can hide it.
 */
export function tipPosition(rect, viewportWidth, boxWidth = TIP_MAX_W, boxHeight = 0, viewportHeight = Infinity) {
  const centre = rect.left + rect.width / 2;
  const maxLeft = Math.max(EDGE, viewportWidth - boxWidth - EDGE);
  const left = Math.min(maxLeft, Math.max(EDGE, centre - boxWidth / 2));
  const below = rect.bottom + GAP;
  const fitsBelow = below + boxHeight <= viewportHeight - EDGE;
  const above = rect.top - GAP - boxHeight;
  const top = fitsBelow || above < EDGE ? below : above;
  return { top, left };
}

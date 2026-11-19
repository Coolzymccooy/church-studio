/**
 * Colours and fonts for the live visualiser canvas, read from the faceplate tokens (a canvas cannot
 * use CSS variables directly). The canvas is a glass window, so it uses the glass and lamp tokens,
 * which are the same in every finish.
 */
export const PALETTE_TOKENS = {
  bg: '--fp-glass',
  text: '--fp-glass-text',
  dim: '--fp-glass-dim',
  grid: '--fp-glass-grid',
  g: '--fp-led-g',
  gDim: '--fp-led-g-dim',
  a: '--fp-led-a',
  aDim: '--fp-led-a-dim',
  r: '--fp-led-r',
  rDim: '--fp-led-r-dim',
  mono: '--fp-font-mono',
};

const FALLBACK = {
  bg: '#0D1110', text: '#D8E6DD', dim: '#7C8C83', grid: '#1E2A24',
  g: '#3DD68C', gDim: '#1C3526', a: '#F2B33D', aDim: '#3A2E14', r: '#F0533F', rDim: '#3A1A15',
  mono: 'monospace',
};

/** Reads each token with `read(name)`; a missing token keeps its fallback. */
export function readVisualiserPalette(read) {
  const out = {};
  for (const [key, token] of Object.entries(PALETTE_TOKENS)) {
    const value = typeof read === 'function' ? String(read(token) ?? '').trim() : '';
    out[key] = value || FALLBACK[key];
  }
  return out;
}

/**
 * Colours and fonts for the editor's waveform canvas, read from the faceplate glass and lamp
 * tokens (a canvas cannot use CSS variables directly).
 */
const TOKENS = {
  panel: ['--fp-glass', '#0D1110'],
  text: ['--fp-glass-dim', '#7C8C83'],
  textBright: ['--fp-glass-text', '#D8E6DD'],
  grid: ['--fp-glass-grid', '#1E2A24'],
  wave: ['--fp-led-g', '#3DD68C'],
  selectionBorder: ['--fp-sel', '#E8E2D0'],
  playhead: ['--fp-glass-text', '#D8E6DD'],
  mono: ['--fp-font-mono', 'monospace'],
};

/** "#rrggbb" or "#rgb" to "rgba(r,g,b,a)"; anything else is returned unchanged. */
export function hexToRgba(hex, alpha) {
  const h = String(hex).trim().replace('#', '');
  const full = h.length === 3 ? h.split('').map((c) => c + c).join('') : h;
  if (!/^[0-9a-fA-F]{6}$/.test(full)) return hex;
  const n = parseInt(full, 16);
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${alpha})`;
}

/** Reads each token with `read(name)`; a missing token keeps its fallback. */
export function readEditorPalette(read) {
  const out = {};
  for (const [key, [token, fallback]] of Object.entries(TOKENS)) {
    const value = typeof read === 'function' ? String(read(token) ?? '').trim() : '';
    out[key] = value || fallback;
  }
  out.selection = hexToRgba(out.selectionBorder, 0.22);
  out.centre = hexToRgba(out.textBright, 0.12);
  return out;
}

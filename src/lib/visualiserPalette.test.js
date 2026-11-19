import test from 'node:test';
import assert from 'node:assert/strict';
import { PALETTE_TOKENS, readVisualiserPalette } from './visualiserPalette.js';

test('readVisualiserPalette reads every token and falls back when one is missing', () => {
  const p = readVisualiserPalette((name) => (name === '--fp-led-g' ? ' #00ff00 ' : ''));
  assert.equal(p.g, '#00ff00');
  assert.equal(Object.keys(p).length, Object.keys(PALETTE_TOKENS).length);
  for (const v of Object.values(p)) assert.ok(v.length > 0);
  assert.ok(readVisualiserPalette(null).bg);
});

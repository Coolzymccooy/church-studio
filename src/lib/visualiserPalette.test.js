import test from 'node:test';
import assert from 'node:assert/strict';
import { PALETTE_TOKENS, barColour, readVisualiserPalette } from './visualiserPalette.js';

test('readVisualiserPalette reads every token and falls back when one is missing', () => {
  const p = readVisualiserPalette((name) => (name === '--fp-led-g' ? ' #00ff00 ' : ''));
  assert.equal(p.g, '#00ff00');
  assert.equal(Object.keys(p).length, Object.keys(PALETTE_TOKENS).length);
  for (const v of Object.values(p)) assert.ok(v.length > 0);
  assert.ok(readVisualiserPalette(null).bg);
});

test('barColour: green for passing voice, amber for rejected, quiet otherwise', () => {
  const p = readVisualiserPalette(() => '');
  assert.equal(barColour(p, { voiceActive: true, gateClosed: false, inVoiceBand: true, level: 90 }), p.g);
  assert.equal(barColour(p, { voiceActive: true, gateClosed: false, inVoiceBand: false, level: 90 }), p.gDim);
  assert.equal(barColour(p, { voiceActive: false, gateClosed: true, inVoiceBand: true, level: 40 }), p.a);
  assert.equal(barColour(p, { voiceActive: false, gateClosed: true, inVoiceBand: false, level: 40 }), p.aDim);
  assert.equal(barColour(p, { voiceActive: false, gateClosed: false, inVoiceBand: true, level: 40 }), p.dim);
  assert.equal(barColour(p, { voiceActive: false, gateClosed: false, inVoiceBand: false, level: 40 }), p.grid);
  assert.notEqual(barColour(p, { voiceActive: false, gateClosed: true, inVoiceBand: false, level: 40 }), p.r);
});

import test from 'node:test';
import assert from 'node:assert/strict';
import { hexToRgba, readEditorPalette } from './editorPalette.js';

test('hexToRgba converts long and short hex and leaves other values alone', () => {
  assert.equal(hexToRgba('#ff8000', 0.5), 'rgba(255,128,0,0.5)');
  assert.equal(hexToRgba('#fff', 1), 'rgba(255,255,255,1)');
  assert.equal(hexToRgba('rgb(1,2,3)', 0.5), 'rgb(1,2,3)');
});

test('readEditorPalette falls back per token and derives the selection fill', () => {
  const p = readEditorPalette((name) => (name === '--fp-sel' ? '#102030' : ''));
  assert.equal(p.selectionBorder, '#102030');
  assert.equal(p.selection, 'rgba(16,32,48,0.22)');
  assert.ok(p.panel.startsWith('#'));
  assert.ok(p.mono.length > 0);
});

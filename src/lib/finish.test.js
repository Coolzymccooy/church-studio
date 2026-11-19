import test from 'node:test';
import assert from 'node:assert/strict';
import { FINISHES, nextFinish, readFinish } from './finish.js';

test('readFinish keeps known finishes and falls back to graphite', () => {
  for (const f of FINISHES) assert.equal(readFinish(f), f);
  assert.equal(readFinish(null), 'graphite');
  assert.equal(readFinish('amber'), 'graphite');
  assert.equal(readFinish(''), 'graphite');
});

test('nextFinish cycles graphite, bone, auto', () => {
  assert.equal(nextFinish('graphite'), 'bone');
  assert.equal(nextFinish('bone'), 'auto');
  assert.equal(nextFinish('auto'), 'graphite');
  assert.equal(nextFinish('violet'), 'bone');
});

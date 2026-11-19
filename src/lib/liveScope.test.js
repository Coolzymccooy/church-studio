import test from 'node:test';
import assert from 'node:assert/strict';
import {
  SCOPE, analyserRange, bandEdges, bandLevels, createScope, dbFraction, freqFraction, gateState, gridLines, stepScope,
} from './liveScope.js';

test('freqFraction is logarithmic and clamped', () => {
  assert.equal(freqFraction(SCOPE.fMin), 0);
  assert.equal(freqFraction(SCOPE.fMax), 1);
  assert.equal(freqFraction(5), 0);
  const mid = Math.sqrt(SCOPE.fMin * SCOPE.fMax);
  assert.ok(Math.abs(freqFraction(mid) - 0.5) < 1e-9);
});

test('dbFraction maps the analyser range and clamps', () => {
  assert.equal(dbFraction(SCOPE.dbMin), 0);
  assert.equal(dbFraction(SCOPE.dbMax), 1);
  assert.equal(dbFraction(-200), 0);
  assert.equal(dbFraction(0), 1);
});

test('bandEdges covers increasing bins, at least one each', () => {
  const bands = bandEdges(48000, 1024);
  assert.equal(bands.length, SCOPE.bands);
  bands.forEach((b, i) => {
    assert.ok(b.hi >= b.lo);
    if (i > 0) assert.ok(b.lo >= bands[i - 1].lo);
    assert.ok(b.hi < 1024);
  });
});

test('bandLevels takes the loudest bin in each band', () => {
  const bytes = new Uint8Array(1024);
  bytes[100] = 255;
  const bands = bandEdges(48000, 1024);
  const levels = bandLevels(bytes, bands);
  const hit = bands.findIndex((b) => b.lo <= 100 && b.hi >= 100);
  assert.equal(levels[hit], 1);
  assert.equal(levels.filter((v) => v > 0).length, bands.filter((b) => b.lo <= 100 && b.hi >= 100).length);
});

test('stepScope: fast rise, slow fall, peak holds then falls, input unchanged', () => {
  const s0 = createScope(1);
  const s1 = stepScope(s0, [1], 0.1);
  assert.equal(s1.levels[0], 1);
  assert.equal(s0.levels[0], 0);
  const s2 = stepScope(s1, [0], 0.1);
  assert.ok(Math.abs(s2.levels[0] - (1 - SCOPE.fallRate * 0.1)) < 1e-9);
  assert.equal(s2.peaks[0], 1);
  let s = s2;
  for (let i = 0; i < 20; i += 1) s = stepScope(s, [0], 0.1);
  assert.ok(s.peaks[0] < 1);
  assert.ok(s.peaks[0] >= s.levels[0]);
});

test('stepScope keeps a bounded timeline', () => {
  let s = createScope(1);
  for (let i = 0; i < 1000; i += 1) s = stepScope(s, [0.5], 1 / 30, { level: 0.5, state: 'voice' });
  assert.equal(s.history.length, Math.round(SCOPE.historyS * 30));
});

test('gateState words', () => {
  assert.equal(gateState({ denoise: true, bypassed: true }), 'bypass');
  assert.equal(gateState({ denoise: false, voiceActive: false }), 'off');
  assert.equal(gateState({ denoise: true, gateClosed: true }), 'held');
  assert.equal(gateState({ denoise: true, gateClosed: false, voiceActive: true }), 'voice');
  assert.equal(gateState({ denoise: true, gateClosed: false, voiceActive: false }), 'open');
});

test('gridLines: every 10 dB inside the range, for both backends', () => {
  assert.deepEqual(gridLines({ min: -100, max: -30 }), [-90, -80, -70, -60, -50, -40]);
  assert.deepEqual(gridLines({ min: -90, max: 0 }), [-80, -70, -60, -50, -40, -30, -20, -10]);
});

test('analyserRange reads the node, else the Web Audio default', () => {
  assert.deepEqual(analyserRange({ minDecibels: -90, maxDecibels: 0 }), { min: -90, max: 0 });
  assert.deepEqual(analyserRange({}), { min: SCOPE.dbMin, max: SCOPE.dbMax });
  assert.deepEqual(analyserRange({ minDecibels: 0, maxDecibels: -10 }), { min: SCOPE.dbMin, max: SCOPE.dbMax });
});

import test from 'node:test';
import assert from 'node:assert/strict';
import { LOUD_MAX, LOUD_MIN, loudFraction, lufsText, pushHistory, targetDelta } from './loudness.js';

test('loudFraction clamps to the display scale', () => {
  assert.equal(loudFraction(LOUD_MIN), 0);
  assert.equal(loudFraction(LOUD_MAX), 1);
  assert.equal(loudFraction(-20), 0.5);
  assert.equal(loudFraction(-90), 0);
  assert.equal(loudFraction(null), 0);
});

test('targetDelta words and tones', () => {
  assert.deepEqual(targetDelta(-14.5, -14), { lu: -0.5, text: 'on target', tone: 'good' });
  assert.equal(targetDelta(-21.9, -14).text, '-7.9 LU');
  assert.equal(targetDelta(-21.9, -14).tone, 'low');
  assert.equal(targetDelta(-12, -14).tone, 'caution');
  assert.equal(targetDelta(-12, -14).text, '+2.0 LU');
  assert.equal(targetDelta(-10, -14).tone, 'fault');
  assert.equal(targetDelta(null, -14).tone, 'idle');
});

test('pushHistory is bounded and does not change its input', () => {
  const h0 = [1, 2];
  const h1 = pushHistory(h0, 3, 2);
  assert.deepEqual(h1, [2, 3]);
  assert.deepEqual(h0, [1, 2]);
  assert.deepEqual(pushHistory([], NaN, 5), [null]);
});

test('lufsText', () => {
  assert.equal(lufsText(-21.94), '-21.9');
  assert.equal(lufsText(null), '--');
});

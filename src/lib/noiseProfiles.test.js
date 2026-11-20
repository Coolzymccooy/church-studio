import test from 'node:test';
import assert from 'node:assert/strict';

import { newNoiseProfileId, nextNoiseProfileNumber } from './noiseProfiles.js';

test('noise profile ids are never reused, even after reset or a restart', () => {
  const a = newNoiseProfileId(1000, () => 0.5);
  const b = newNoiseProfileId(1000, () => 0.25);
  const c = newNoiseProfileId(2000, () => 0.5);
  assert.notEqual(a, b);
  assert.notEqual(a, c);
  assert.match(a, /^np-[a-z0-9]+-[a-z0-9]+$/);
  assert.notEqual(newNoiseProfileId(), newNoiseProfileId());
});

test('the next profile number follows the saved profiles', () => {
  assert.equal(nextNoiseProfileNumber([]), 1);
  assert.equal(nextNoiseProfileNumber(undefined), 1);
  assert.equal(nextNoiseProfileNumber([{ label: 'Noise Profile 2' }, { label: 'Noise Profile 5' }]), 6);
  assert.equal(nextNoiseProfileNumber([{ label: 'Hall' }, { label: 'Noise Profile 1' }]), 2);
});

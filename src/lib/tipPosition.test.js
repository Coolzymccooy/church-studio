import test from 'node:test';
import assert from 'node:assert/strict';
import { TIP_MAX_W, tipPosition } from './tipPosition.js';

const rect = (left, top = 100, size = 16) => ({ left, top, width: size, height: size, bottom: top + size });

test('centres the box under the key when there is room', () => {
  assert.deepEqual(tipPosition(rect(500), 1440), { top: 122, left: 508 - TIP_MAX_W / 2 });
});

test('keeps the box inside the left and right edges', () => {
  assert.equal(tipPosition(rect(10), 1440).left, 8);
  assert.equal(tipPosition(rect(1420), 1440).left, 1440 - TIP_MAX_W - 8);
});

test('a viewport narrower than the box pins it to the left edge', () => {
  assert.equal(tipPosition(rect(100), 200).left, 8);
});

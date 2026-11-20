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

test('flips above the key when the box would leave the bottom of the window', () => {
  const key = { left: 500, top: 860, width: 60, height: 24, bottom: 884 };
  assert.deepEqual(tipPosition(key, 1440, 240, 60, 900), { top: 860 - 6 - 60, left: 530 - 120 });
  const roomy = { left: 500, top: 100, width: 60, height: 24, bottom: 124 };
  assert.equal(tipPosition(roomy, 1440, 240, 60, 900).top, 130);
});

test('stays below when there is no room above either', () => {
  const key = { left: 10, top: 20, width: 16, height: 16, bottom: 36 };
  assert.equal(tipPosition(key, 1440, 240, 200, 120).top, 42);
});

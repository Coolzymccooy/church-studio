import assert from 'node:assert/strict';
import test from 'node:test';
import { addressLabel, countdownLabel, PAIR_SECONDS, seenLabel } from './remoteMixerPanel.js';

test('a paired device reads as live while it streams, then by how long ago it was seen', () => {
  assert.equal(seenLabel(0), 'live');
  assert.equal(seenLabel(4), 'live');
  assert.equal(seenLabel(45), '45 s ago');
  assert.equal(seenLabel(340), '5 min ago');
  assert.equal(seenLabel(7300), '2 h ago');
  assert.equal(seenLabel(undefined), '');
});

test('the pairing countdown reads in minutes and seconds', () => {
  assert.equal(PAIR_SECONDS, 120);
  assert.equal(countdownLabel(120), '2:00');
  assert.equal(countdownLabel(65), '1:05');
  assert.equal(countdownLabel(0), '0:00');
  assert.equal(countdownLabel(-3), '0:00');
});

test('the address drops the scheme so the host reads first', () => {
  assert.equal(addressLabel('https://oluwaseguns-macbook-pro.tailbc77ed.ts.net:4475'), 'oluwaseguns-macbook-pro.tailbc77ed.ts.net:4475');
  assert.equal(addressLabel('https://192.168.1.20:4475/'), '192.168.1.20:4475');
  assert.equal(addressLabel(''), '');
});

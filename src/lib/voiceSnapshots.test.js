import test from 'node:test';
import assert from 'node:assert/strict';

import { SNAPSHOTS_KEY, nextSnapshotId, parseSnapshots, serializeSnapshots } from './voiceSnapshots.js';

const good = {
  id: 2,
  label: 'Snapshot 2',
  features: { lookaheadGate: true, deEsser: false },
  speakerPreset: 'pastor',
  selectedNoiseProfileId: 'np-1',
  noiseFloorThreshold: -48,
  inputGainValue: 1.2,
};

test('snapshots survive a save and load round trip', () => {
  assert.deepEqual(parseSnapshots(serializeSnapshots([good])), [good]);
  assert.equal(SNAPSHOTS_KEY, 'tiwaton:snapshots');
});

test('missing or broken storage loads as no snapshots', () => {
  assert.deepEqual(parseSnapshots(null), []);
  assert.deepEqual(parseSnapshots(''), []);
  assert.deepEqual(parseSnapshots('{not json'), []);
  assert.deepEqual(parseSnapshots('{"id":1}'), []);
});

test('entries without an id or features are dropped, unknown fields are not kept', () => {
  const raw = JSON.stringify([
    good,
    { label: 'no id', features: {} },
    { id: 3, label: 'no features' },
    { ...good, id: 4, label: 'Snapshot 4', extra: 'x', speakerPreset: 'nonsense' },
  ]);
  const list = parseSnapshots(raw);
  assert.deepEqual(list.map((s) => s.id), [2, 4]);
  assert.equal('extra' in list[1], false);
  assert.equal(list[1].speakerPreset, 'balanced', 'an unknown preset falls back to balanced');
});

test('feature flags are kept only when they are true or false', () => {
  const raw = JSON.stringify([{ ...good, features: { a: true, b: 'yes', c: false } }]);
  assert.deepEqual(parseSnapshots(raw)[0].features, { a: true, c: false });
});

test('numbers that are not finite become null so apply keeps a safe value', () => {
  const raw = JSON.stringify([{ ...good, noiseFloorThreshold: 'loud', inputGainValue: null }]);
  const [s] = parseSnapshots(raw);
  assert.equal(s.noiseFloorThreshold, null);
  assert.equal(s.inputGainValue, null);
});

test('the next snapshot number follows the highest saved one', () => {
  assert.equal(nextSnapshotId([]), 1);
  assert.equal(nextSnapshotId([{ id: 2 }, { id: 7 }, { id: 3 }]), 8);
});

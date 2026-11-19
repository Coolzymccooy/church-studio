import test from 'node:test';
import assert from 'node:assert/strict';
import { obsLampState, outputKey, stepScene } from './obsPlate.js';

test('stepScene wraps both ways and handles unknown or empty lists', () => {
  const s = ['a', 'b', 'c'];
  assert.equal(stepScene(s, 'b', 1), 'c');
  assert.equal(stepScene(s, 'c', 1), 'a');
  assert.equal(stepScene(s, 'a', -1), 'c');
  assert.equal(stepScene(s, 'zzz', 1), 'a');
  assert.equal(stepScene(s, null, -1), 'c');
  assert.equal(stepScene([], 'a', 1), null);
});

test('obsLampState', () => {
  assert.deepEqual(obsLampState({ connected: true, obsVersion: '30.2' }), { tone: 'g', word: 'obs 30.2' });
  assert.deepEqual(obsLampState({ connecting: true }), { tone: 'a', word: 'connecting' });
  assert.deepEqual(obsLampState({ enabled: true }), { tone: 'off', word: 'not connected' });
  assert.deepEqual(obsLampState({}), { tone: 'off', word: 'off' });
});

test('outputKey legends', () => {
  assert.deepEqual(outputKey('stream', false), { text: 'go live', tc: '' });
  assert.deepEqual(outputKey('stream', true, '00:01:02'), { text: 'on air', tc: '00:01:02' });
  assert.deepEqual(outputKey('record', true, null), { text: 'recording', tc: '' });
});

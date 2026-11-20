import assert from 'node:assert/strict';
import test from 'node:test';
import { createSoundcheckController, durationLabel } from './soundcheck.js';

test('soundcheck controls preserve strip and sample-position argument types', async () => {
  const calls = [];
  const desk = createSoundcheckController((cmd, args) => calls.push([cmd, args]));
  await desk.load('/session/session.json');
  await desk.source(3, true);
  await desk.locate(1.333);
  await desk.loop(true);
  assert.deepEqual(calls, [
    ['soundcheck_load', { path: '/session/session.json' }],
    ['soundcheck_set_source', { index: 3, playback: true }],
    ['soundcheck_locate', { seconds: 1.333 }],
    ['soundcheck_set_loop', { enabled: true }],
  ]);
});

test('the transport clock displays elapsed time with tabular fields', () => {
  assert.equal(durationLabel(0), '0:00');
  assert.equal(durationLabel(75.95), '1:15');
  assert.equal(durationLabel(3650), '1:00:50');
});

test('armed playback stays visible after the transport stops', async () => {
  const { soundcheckLamp } = await import('./soundcheck.js');
  assert.equal(soundcheckLamp({ selected: 2, playing: false }), 'a');
  assert.equal(soundcheckLamp({ selected: 2, playing: true }), 'g');
  assert.equal(soundcheckLamp({ selected: 0, playing: false }), 'off');
});

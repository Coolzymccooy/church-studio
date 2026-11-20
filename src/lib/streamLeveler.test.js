import test from 'node:test';
import assert from 'node:assert/strict';
import { levelerTarget, syncStreamLeveler, LEVELER_TARGET_DEFAULT } from './streamLeveler.js';
import { createMixerController, createMockMixerController } from './mixerEngine.js';

test('levelerTarget clamps to the contract range and defaults bad input', () => {
  assert.equal(levelerTarget(-14), -14);
  assert.equal(levelerTarget(-50), -30);
  assert.equal(levelerTarget(0), -10);
  assert.equal(levelerTarget(Number.NaN), LEVELER_TARGET_DEFAULT);
  assert.equal(levelerTarget(undefined), LEVELER_TARGET_DEFAULT);
  assert.equal(levelerTarget('-16'), LEVELER_TARGET_DEFAULT);
});

test('syncStreamLeveler sends the target, then the switch, to the Stream bus', async () => {
  const calls = [];
  const controller = createMixerController({
    invoke: async (command, payload) => { calls.push({ command, payload }); return null; },
    listen: async () => () => {},
  });
  assert.equal(await syncStreamLeveler(controller, { on: true, target: -16 }), null);
  assert.deepEqual(calls, [
    { command: 'mixer_set_bus_param', payload: { bus: 'stream', key: 'leveler_target_lufs', value: -16 } },
    { command: 'mixer_set_bus_bool', payload: { bus: 'stream', key: 'leveler_enabled', value: true } },
  ]);
});

test('syncStreamLeveler reports a failure instead of throwing', async () => {
  const controller = createMixerController({
    invoke: async () => { throw new Error('unknown bus key'); },
    listen: async () => () => {},
  });
  assert.equal(await syncStreamLeveler(controller, { on: false, target: -14 }), 'unknown bus key');
});

test('the demo mixer keeps the leveler keys on the Stream bus', async () => {
  const controller = createMockMixerController();
  const before = await controller.getState();
  const stream0 = before.buses.find((b) => b.id === 'stream');
  assert.equal(stream0.leveler_enabled, false);
  assert.equal(stream0.leveler_target_lufs, -14);
  await syncStreamLeveler(controller, { on: true, target: -40 });
  const stream = (await controller.getState()).buses.find((b) => b.id === 'stream');
  assert.equal(stream.leveler_enabled, true);
  assert.equal(stream.leveler_target_lufs, -30);
});

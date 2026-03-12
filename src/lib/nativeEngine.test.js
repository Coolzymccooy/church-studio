import test from 'node:test';
import assert from 'node:assert/strict';

import {
  captureNativeNoiseProfile,
  createNativeEngineController,
  describeNativeEngineError,
  getNativeEngineStatus,
  subscribeToNativeMeters,
  syncNativeParams,
} from './nativeEngine.js';

test('syncNativeParams writes the expected native DSP contract', async () => {
  const calls = [];
  const invoke = async (command, payload) => {
    calls.push({ command, payload });
    return null;
  };

  await syncNativeParams(invoke, {
    features: { denoise: true, dynamicDeEsser: true, dereverb: false },
    isBypassed: false,
    noiseFloorThreshold: -42,
    inputGainValue: 2,
  });

  assert.equal(calls.length, 8);
  assert.deepEqual(calls[0], {
    command: 'set_param_bool',
    payload: { key: 'gate_enabled', value: true },
  });
  assert.deepEqual(calls.at(-1), {
    command: 'set_param',
    payload: { key: 'gain_db', value: 20 * Math.log10(2) },
  });
});

test('native engine controller restarts through stop then start', async () => {
  const calls = [];
  const controller = createNativeEngineController({
    invoke: async (command, payload) => {
      calls.push({ command, payload });
      return { ok: true };
    },
    listen: async () => () => {},
  });

  await controller.restart({
    inputId: 'mic-1',
    outputId: 'monitor-1',
    broadcastBus: 'broadcast-1',
  });

  assert.deepEqual(calls.map((call) => call.command), [
    'stop_audio_engine',
    'start_audio_engine',
  ]);
  assert.deepEqual(calls[1].payload, {
    input_device: 'mic-1',
    monitor_output_device: 'monitor-1',
    broadcast_output_device: 'broadcast-1',
  });
});

test('subscribeToNativeMeters disposes late listeners safely', async () => {
  let disposed = false;
  const subscription = subscribeToNativeMeters(
    async (_eventName, handler) => {
      handler({ payload: { input_db: -20 } });
      return () => {
        disposed = true;
      };
    },
    () => {},
  );

  await subscription.ready;
  subscription.dispose();

  assert.equal(disposed, true);
});

test('native engine helpers proxy capture and status commands', async () => {
  const commands = [];
  const invoke = async (command) => {
    commands.push(command);
    return { running: command === 'engine_status' };
  };

  const status = await getNativeEngineStatus(invoke);
  await captureNativeNoiseProfile(invoke);

  assert.deepEqual(commands, ['engine_status', 'capture_noise_profile']);
  assert.equal(status.running, true);
});

test('describeNativeEngineError prefers useful native error text', () => {
  assert.equal(
    describeNativeEngineError({ message: 'No common sample rate is available' }),
    'No common sample rate is available',
  );
  assert.equal(
    describeNativeEngineError({ error: 'monitor output device not found' }),
    'monitor output device not found',
  );
  assert.equal(
    describeNativeEngineError({}, 'Could not start Rust audio engine.'),
    'Could not start Rust audio engine.',
  );
});

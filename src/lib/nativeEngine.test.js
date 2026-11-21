import test from 'node:test';
import assert from 'node:assert/strict';

import {
  captureNativeNoiseProfile,
  createNativeEngineController,
  describeNativeEngineError,
  getNativeEngineStatus,
  learnNativeMicFingerprint,
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

  assert.equal(calls.length, 10);
  assert.deepEqual(calls[0], {
    command: 'set_param_bool',
    payload: { key: 'gate_enabled', value: true },
  });
  assert.ok(calls.some((c) => c.payload.key === 'neural_denoise' && c.payload.value === true));
  assert.deepEqual(calls.at(-1), {
    command: 'set_param',
    payload: { key: 'gain_db', value: 20 * Math.log10(2) },
  });
});

test('syncNativeParams turns neural denoise on for Voice Pattern Cleanse and off with both features off', async () => {
  const sent = async (features) => {
    const calls = [];
    await syncNativeParams(async (command, payload) => { calls.push(payload); }, {
      features,
      isBypassed: false,
      noiseFloorThreshold: -42,
      inputGainValue: 1,
    });
    return calls.find((p) => p.key === 'neural_denoise').value;
  };

  assert.equal(await sent({ denoise: false, voicePattern: true }), true);
  assert.equal(await sent({ denoise: false, voicePattern: false }), false);
});

test('syncNativeParams sends the Hyper gate mode: speech only or balanced', async () => {
  const sent = async (gateMode) => {
    const calls = [];
    await syncNativeParams(async (command, payload) => { calls.push({ command, payload }); }, {
      features: { denoise: true },
      isBypassed: false,
      noiseFloorThreshold: -45,
      inputGainValue: 1,
      gateMode,
    });
    return calls.find((c) => c.payload.key === 'gate_speech_only');
  };

  assert.deepEqual(await sent('speech'), {
    command: 'set_param_bool',
    payload: { key: 'gate_speech_only', value: true },
  });
  assert.equal((await sent('balanced')).payload.value, false);
  assert.equal((await sent(undefined)).payload.value, false);
});

test('learn mic calls the native learn command and passes its error through', async () => {
  const calls = [];
  await learnNativeMicFingerprint(async (command, payload) => { calls.push({ command, payload }); });
  assert.deepEqual(calls, [{ command: 'learn_mic_fingerprint', payload: undefined }]);

  await assert.rejects(
    learnNativeMicFingerprint(async () => { throw 'Not enough speech heard.'; }),
    (err) => err === 'Not enough speech heard.',
  );

  const controller = createNativeEngineController({
    invoke: async (command) => { calls.push({ command }); },
    listen: async () => () => {},
  });
  await controller.learnMicFingerprint();
  assert.equal(calls.at(-1).command, 'learn_mic_fingerprint');
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
    inputDevice: 'mic-1',
    monitorOutputDevice: 'monitor-1',
    broadcastOutputDevice: 'broadcast-1',
    mainOutputId: null,
    monitor2OutputId: null,
    monitor3OutputId: null,
    outputPairs: { main: 0, stream: 0, monitor: 0, monitor2: 0, monitor3: 0 },
    inputChannels: { first: 0, count: null },
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

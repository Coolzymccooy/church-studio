import test from 'node:test';
import assert from 'node:assert/strict';

import {
  computerAudioFromEngineStatus,
  computerAudioLamp,
  createComputerAudioClient,
  normalizeComputerAudio,
} from './computerAudio.js';

test('normalize fills safe defaults', () => {
  assert.deepEqual(normalizeComputerAudio(undefined), {
    enabled: false,
    permission: 'unknown',
    running: false,
    restartRequired: false,
    saveError: null,
  });
  const result = normalizeComputerAudio({
    enabled: true,
    permission: 'denied',
    running: true,
    restart_required: true,
    save_error: 'disk full',
  });
  assert.equal(result.enabled, true);
  assert.equal(result.permission, 'denied');
  assert.equal(result.restartRequired, true);
  assert.equal(result.saveError, 'disk full');
  assert.equal(normalizeComputerAudio({ permission: 'maybe' }).permission, 'unknown');
});

test('lamp shows the state in a few words', () => {
  const base = normalizeComputerAudio({});
  assert.deepEqual(computerAudioLamp(base), { tone: 'off', word: 'Off' });
  assert.deepEqual(computerAudioLamp({ ...base, enabled: true }), { tone: 'g', word: 'On' });
  assert.deepEqual(
    computerAudioLamp({ ...base, enabled: true, restartRequired: true }),
    { tone: 'a', word: 'Restart to apply' },
  );
  // Turning it off also needs a restart.
  assert.equal(computerAudioLamp({ ...base, restartRequired: true }).word, 'Restart to apply');
  assert.deepEqual(
    computerAudioLamp({ ...base, enabled: true, permission: 'denied', restartRequired: true }),
    { tone: 'r', word: 'Needs permission' },
  );
  // Denied only matters while it is on.
  assert.equal(computerAudioLamp({ ...base, permission: 'denied' }).word, 'Off');
  assert.deepEqual(computerAudioLamp({ ...base, saveError: 'x' }), { tone: 'a', word: 'Not saved' });
  assert.deepEqual(computerAudioLamp(base, 'boom'), { tone: 'r', word: 'Not changed' });
  for (const lamp of [computerAudioLamp(base), computerAudioLamp({ ...base, restartRequired: true })]) {
    assert.ok(lamp.word.split(' ').length <= 3);
  }
});

test('client calls the commands and normalizes the results', async () => {
  const calls = [];
  const invoke = async (command, args) => {
    calls.push([command, args]);
    return command === 'computer_audio_set'
      ? { enabled: args.enabled, permission: 'granted', running: true, restart_required: true }
      : { enabled: false, permission: 'granted' };
  };
  const client = createComputerAudioClient(invoke);
  assert.equal((await client.get()).enabled, false);
  const set = await client.set(1);
  assert.equal(set.enabled, true);
  assert.equal(set.restartRequired, true);
  assert.deepEqual(calls, [['computer_audio_get', undefined], ['computer_audio_set', { enabled: true }]]);
});

test('live status reads engine_status.computer_audio', () => {
  assert.deepEqual(computerAudioFromEngineStatus(undefined), {
    enabled: false, active: false, mutesSpeakers: false, strips: [], droppedSamples: 0, underruns: 0,
  });
  const running = computerAudioFromEngineStatus({
    computer_audio: {
      enabled: true, running: true, mutes_original_output: true, strips: [2, 3], dropped_samples: 4, underruns: 1,
    },
  });
  assert.deepEqual(running, {
    enabled: true, active: true, mutesSpeakers: true, strips: [2, 3], droppedSamples: 4, underruns: 1,
  });
  // Enabled but not capturing (engine stopped or capture failed): nothing is muted.
  const idle = computerAudioFromEngineStatus({ computer_audio: { enabled: true, running: false, mutes_original_output: true } });
  assert.equal(idle.active, false);
  assert.equal(idle.mutesSpeakers, false);
});

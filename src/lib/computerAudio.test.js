import test from 'node:test';
import assert from 'node:assert/strict';

import {
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

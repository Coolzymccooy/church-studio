import test from 'node:test';
import assert from 'node:assert/strict';

import {
  BUFFER_STEPS,
  DEFAULT_BUFFER_MS,
  MAX_BUFFER_MS,
  MIN_BUFFER_MS,
  DEFAULT_PORT,
  MAX_NET_INPUTS,
  MIN_PASSPHRASE_CHARS,
  createNetAudioClient,
  nextBufferMs,
  netAudioFromEngineStatus,
  passphraseProblem,
  sanitizeNetAudio,
} from './netAudio.js';

test('sanitizeNetAudio clamps every field like the engine', () => {
  const clean = sanitizeNetAudio({
    receive: { enabled: true, port: 0, slots: 9, bufferMs: 1 },
    send: { enabled: 1, target: '  church.local:47810\u0007 ', bus: ' STREAM ', stereo: 'yes', name: 'x'.repeat(50) },
    passphrase: ' secret-room ',
  });
  assert.deepEqual(clean.receive, { enabled: true, port: DEFAULT_PORT, slots: MAX_NET_INPUTS, bufferMs: 15 });
  assert.equal(clean.send.enabled, true);
  assert.equal(clean.send.target, 'church.local:47810');
  assert.equal(clean.send.bus, 'stream');
  assert.equal(clean.send.stereo, true);
  assert.equal(clean.send.name.length, 32);
  assert.equal(clean.passphrase, 'secret-room');
});

test('sanitizeNetAudio fills defaults for missing or bad input', () => {
  const clean = sanitizeNetAudio(null);
  assert.deepEqual(clean.receive, { enabled: false, port: DEFAULT_PORT, slots: 2, bufferMs: DEFAULT_BUFFER_MS });
  assert.deepEqual(clean.send, { enabled: false, target: '', bus: 'main', stereo: false, name: '' });
  assert.equal(clean.passphrase, '');
  assert.equal(sanitizeNetAudio({ send: { bus: 'aux' } }).send.bus, 'main');
  assert.equal(sanitizeNetAudio({ send: { bus: 'Monitor3' } }).send.bus, 'monitor3');
  assert.equal(sanitizeNetAudio({ receive: { port: 70000 } }).receive.port, DEFAULT_PORT);
});

test('passphraseProblem only speaks when something is on', () => {
  const off = sanitizeNetAudio({ passphrase: 'short' });
  assert.equal(passphraseProblem(off), null);
  const on = sanitizeNetAudio({ receive: { enabled: true }, passphrase: 'short' });
  assert.match(passphraseProblem(on), new RegExp(String(MIN_PASSPHRASE_CHARS)));
  assert.equal(passphraseProblem({ ...on, passphrase: 'long-enough' }), null);
});

test('netAudioFromEngineStatus reads peers and the sender', () => {
  const status = netAudioFromEngineStatus({
    net_audio: {
      receiving: true,
      port: 47810,
      error: null,
      badPackets: 3,
      peers: [
        { strip: 2, label: 'Remote 1', connected: true, name: 'Pastor', address: '100.1.2.3:5000', bufferMs: 21.4, jitterMs: 0.8, rttMs: 12.2, latencyMs: 38.6, lostPackets: 1, underruns: 0, senderRate: 48000, rateMismatchPackets: 0 },
        { strip: 3, label: 'Remote 2', connected: false },
        'junk',
      ],
      sending: { connected: true, address: '100.1.2.4:47810', rttMs: 11.5, packetsSent: 400, sendErrors: 0, droppedSamples: 0 },
    },
  });
  assert.equal(status.receiving, true);
  assert.equal(status.badPackets, 3);
  assert.equal(status.peers.length, 2);
  assert.equal(status.peers[0].name, 'Pastor');
  assert.equal(status.peers[0].latencyMs, 38.6);
  assert.equal(status.peers[1].connected, false);
  assert.equal(status.peers[1].latencyMs, 0);
  assert.equal(status.sending.connected, true);
  assert.equal(status.sending.rttMs, 11.5);
  assert.equal(status.sending.refused, null);
  const full = netAudioFromEngineStatus({ net_audio: { sending: { connected: false, refused: 'full' } } });
  assert.equal(full.sending.refused, 'full');
  const odd = netAudioFromEngineStatus({ net_audio: { sending: { refused: 'other' } } });
  assert.equal(odd.sending.refused, null);
  const stopped = netAudioFromEngineStatus({ running: false });
  assert.deepEqual(stopped, { receiving: false, port: null, error: null, badPackets: 0, peers: [], sending: null });
});

test('createNetAudioClient sends the camelCase config and reads the result', async () => {
  const calls = [];
  const invoke = async (command, args) => {
    calls.push([command, args]);
    if (command === 'net_audio_set') {
      return { config: args.config, running: true, restart_required: true, problem: null, save_error: 'disk full' };
    }
    if (command === 'net_audio_get') return { receive: { enabled: true }, passphrase: 'abcd-efgh-ijkl' };
    if (command === 'net_audio_new_passphrase') return 'mnpq-rstu-vwxy';
    if (command === 'net_audio_local_addresses') return ['192.168.1.20', 7, '100.75.12.100'];
    return {};
  };
  const client = createNetAudioClient(invoke);
  const got = await client.get();
  assert.equal(got.receive.enabled, true);
  const result = await client.set({ ...got, receive: { ...got.receive, slots: 3 } });
  assert.equal(calls[1][0], 'net_audio_set');
  assert.equal(calls[1][1].config.receive.slots, 3);
  assert.equal(calls[1][1].config.receive.bufferMs, DEFAULT_BUFFER_MS);
  assert.equal(result.restartRequired, true);
  assert.equal(result.saveError, 'disk full');
  assert.equal(await client.newPassphrase(), 'mnpq-rstu-vwxy');
  assert.deepEqual(await client.localAddresses(), ['192.168.1.20', '100.75.12.100']);
});

test('buffer steps are all valid settings and the cycle always moves', () => {
  for (const step of BUFFER_STEPS) {
    assert.equal(sanitizeNetAudio({ receive: { bufferMs: step } }).receive.bufferMs, step);
  }
  assert.equal(BUFFER_STEPS[0], MIN_BUFFER_MS);
  assert.ok(BUFFER_STEPS.includes(80));
  let value = BUFFER_STEPS[0];
  const seen = new Set();
  for (let i = 0; i < BUFFER_STEPS.length; i += 1) {
    seen.add(value);
    value = nextBufferMs(value);
  }
  assert.equal(seen.size, BUFFER_STEPS.length);
  // A saved value between steps moves to the next larger step.
  assert.equal(nextBufferMs(25), 30);
  assert.equal(nextBufferMs(MAX_BUFFER_MS), BUFFER_STEPS[0]);
});

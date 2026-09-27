import test from 'node:test';
import assert from 'node:assert/strict';

import {
  applyStripChange,
  clampStripValue,
  createCoalescer,
  createMixerController,
  createMockMixerController,
  faderDbToPosition,
  formatDb,
  positionToFaderDb,
  validateSceneName,
} from './mixerEngine.js';

// ── Fader law ────────────────────────────────────────────────────────────────

test('positionToFaderDb: 0 dB sits at 75% travel, -90 at bottom, +10 at top', () => {
  assert.equal(positionToFaderDb(0), -90);
  assert.equal(positionToFaderDb(0.75), 0);
  assert.equal(positionToFaderDb(1), 10);
});

test('faderDbToPosition is the inverse of positionToFaderDb', () => {
  assert.equal(faderDbToPosition(-90), 0);
  assert.equal(faderDbToPosition(0), 0.75);
  assert.equal(faderDbToPosition(10), 1);
});

test('fader law round-trips across the full range', () => {
  for (const db of [-90, -60, -30, -12, -6, -1, 0, 1, 5, 10]) {
    const position = faderDbToPosition(db);
    const back = positionToFaderDb(position);
    assert.ok(Math.abs(back - db) < 1e-9, `expected ${db}, got ${back}`);
  }
  for (const position of [0, 0.25, 0.5, 0.75, 0.9, 1]) {
    const db = positionToFaderDb(position);
    const back = faderDbToPosition(db);
    assert.ok(Math.abs(back - position) < 1e-9, `expected ${position}, got ${back}`);
  }
});

test('fader law clamps out-of-range input', () => {
  assert.equal(positionToFaderDb(-1), -90);
  assert.equal(positionToFaderDb(2), 10);
  assert.equal(faderDbToPosition(-999), 0);
  assert.equal(faderDbToPosition(999), 1);
});

// ── formatDb ─────────────────────────────────────────────────────────────────

test('formatDb shows -inf at or below -90 dB', () => {
  assert.equal(formatDb(-90), '-inf');
  assert.equal(formatDb(-91), '-inf');
  assert.equal(formatDb(-200), '-inf');
});

test('formatDb shows one decimal place, with a + sign for zero/positive', () => {
  assert.equal(formatDb(-3), '-3.0');
  assert.equal(formatDb(-3.26), '-3.3');
  assert.equal(formatDb(0), '+0.0');
  assert.equal(formatDb(6.5), '+6.5');
});

// ── clampStripValue ──────────────────────────────────────────────────────────

test('clampStripValue enforces the contract ranges', () => {
  assert.equal(clampStripValue('trim_db', 100), 40);
  assert.equal(clampStripValue('trim_db', -100), -20);
  assert.equal(clampStripValue('hpf_hz', 1), 20);
  assert.equal(clampStripValue('hpf_hz', 5000), 400);
  assert.equal(clampStripValue('gate_threshold_db', 10), 0);
  assert.equal(clampStripValue('eq_low_db', -20), -15);
  assert.equal(clampStripValue('comp_ratio', 0), 1);
  assert.equal(clampStripValue('comp_ratio', 50), 20);
  assert.equal(clampStripValue('pan', -5), -1);
  assert.equal(clampStripValue('pan', 5), 1);
  assert.equal(clampStripValue('fader_db', 50), 10);
  assert.equal(clampStripValue('fader_db', -500), -90);
  assert.equal(clampStripValue('limiter_ceiling_db', 5), 0);
  assert.equal(clampStripValue('limiter_ceiling_db', -50), -12);
});

test('clampStripValue passes through values for keys with no known range', () => {
  assert.equal(clampStripValue('mute', true), true);
  assert.equal(clampStripValue('unknown_key', 42), 42);
});

test('clampStripValue falls back to the range minimum for non-finite input', () => {
  assert.equal(clampStripValue('fader_db', NaN), -90);
  assert.equal(clampStripValue('pan', undefined), -1);
});

// ── validateSceneName ────────────────────────────────────────────────────────

test('validateSceneName accepts letters, digits, space, - and _, 1-40 chars', () => {
  assert.equal(validateSceneName('Sermon'), null);
  assert.equal(validateSceneName('Worship Set-2_Final'), null);
  assert.equal(validateSceneName('a'), null);
  assert.equal(validateSceneName('a'.repeat(40)), null);
});

test('validateSceneName rejects empty, too-long or disallowed characters', () => {
  assert.ok(validateSceneName(''));
  assert.ok(validateSceneName('a'.repeat(41)));
  assert.ok(validateSceneName('../etc/passwd'));
  assert.ok(validateSceneName('name/with/slash'));
  assert.ok(validateSceneName(null));
  assert.ok(validateSceneName(undefined));
});

// ── applyStripChange reducer ─────────────────────────────────────────────────

function makeState() {
  return {
    inputChannels: 2,
    running: true,
    strips: [
      { index: 0, name: 'Ch 1', fader_db: 0, mute: false, voice_chain: true, pan: 0 },
      { index: 1, name: 'Ch 2', fader_db: 0, mute: false, voice_chain: false, pan: 0 },
    ],
    buses: [{ id: 'main', fader_db: 0, mute: false, limiter_ceiling_db: -1 }],
  };
}

test('applyStripChange returns a new state without mutating the original', () => {
  const state = makeState();
  const next = applyStripChange(state, 1, 'fader_db', -6);

  assert.notEqual(next, state);
  assert.notEqual(next.strips, state.strips);
  assert.equal(state.strips[1].fader_db, 0, 'original state must be untouched');
  assert.equal(next.strips[1].fader_db, -6);
  assert.equal(next.strips[0], state.strips[0], 'untouched strips are reused, not recreated');
});

test('applyStripChange clamps numeric values per the contract range', () => {
  const state = makeState();
  const next = applyStripChange(state, 0, 'pan', 5);
  assert.equal(next.strips[0].pan, 1);
});

test('applyStripChange coerces boolean strip keys', () => {
  const state = makeState();
  const next = applyStripChange(state, 1, 'mute', 1);
  assert.equal(next.strips[1].mute, true);
});

test('applyStripChange enforces voice_chain exclusivity', () => {
  const state = makeState();
  const next = applyStripChange(state, 1, 'voice_chain', true);

  assert.equal(next.strips[0].voice_chain, false, 'previous holder is turned off');
  assert.equal(next.strips[1].voice_chain, true);
});

test('applyStripChange leaves other strips alone when voice_chain is turned off', () => {
  const state = makeState();
  const next = applyStripChange(state, 0, 'voice_chain', false);

  assert.equal(next.strips[0].voice_chain, false);
  assert.equal(next.strips[1].voice_chain, false);
});

// ── createMixerController — exact command names and argument shapes ─────────

function recordingInvoke() {
  const calls = [];
  const invoke = async (command, payload) => {
    calls.push({ command, payload });
    return null;
  };
  return { calls, invoke };
}

test('createMixerController wraps every contract command with the exact names/args', async () => {
  const { calls, invoke } = recordingInvoke();
  const listen = async () => () => {};
  const controller = createMixerController({ invoke, listen });

  await controller.getState();
  await controller.setStripParam(2, 'fader_db', -6.5);
  await controller.setStripBool(2, 'mute', true);
  await controller.renameStrip(2, 'Choir L');
  await controller.setBusParam('main', 'fader_db', -3);
  await controller.setBusBool('main', 'mute', true);
  await controller.listScenes();
  await controller.saveScene('Sermon');
  await controller.loadScene('Sermon');
  await controller.deleteScene('Sermon');

  assert.deepEqual(calls, [
    { command: 'mixer_state', payload: undefined },
    { command: 'mixer_set_strip_param', payload: { index: 2, key: 'fader_db', value: -6.5 } },
    { command: 'mixer_set_strip_bool', payload: { index: 2, key: 'mute', value: true } },
    { command: 'mixer_rename_strip', payload: { index: 2, name: 'Choir L' } },
    { command: 'mixer_set_bus_param', payload: { bus: 'main', key: 'fader_db', value: -3 } },
    { command: 'mixer_set_bus_bool', payload: { bus: 'main', key: 'mute', value: true } },
    { command: 'mixer_list_scenes', payload: undefined },
    { command: 'mixer_save_scene', payload: { name: 'Sermon' } },
    { command: 'mixer_load_scene', payload: { name: 'Sermon' } },
    { command: 'mixer_delete_scene', payload: { name: 'Sermon' } },
  ]);
});

test('createMixerController.subscribeMeters listens on mixer-meters and disposes cleanly', async () => {
  const listenCalls = [];
  let unlistenCalled = false;
  const listen = async (event, cb) => {
    listenCalls.push(event);
    listen.cb = cb;
    return () => { unlistenCalled = true; };
  };
  const controller = createMixerController({ invoke: async () => null, listen });

  const received = [];
  const sub = controller.subscribeMeters((payload) => received.push(payload));
  await sub.ready;

  assert.deepEqual(listenCalls, ['mixer-meters']);
  listen.cb({ payload: { strips: [], buses: [] } });
  assert.equal(received.length, 1);

  sub.dispose();
  assert.equal(unlistenCalled, true);
});

test('createMixerController.subscribeMeters disposed before ready still unlistens', async () => {
  let unlistenCalled = false;
  const listen = async () => () => { unlistenCalled = true; };
  const controller = createMixerController({ invoke: async () => null, listen });

  const sub = controller.subscribeMeters(() => {});
  sub.dispose();
  await sub.ready;

  assert.equal(unlistenCalled, true);
});

// ── createMockMixerController ────────────────────────────────────────────────

test('mock controller starts with 8 strips, strip 0 has voice_chain on', async () => {
  const controller = createMockMixerController();
  const state = await controller.getState();

  assert.equal(state.strips.length, 8);
  assert.deepEqual(
    state.strips.map((s) => s.name),
    ['Pastor', 'Lectern', 'Choir L', 'Choir R', 'Keys', 'Bass', 'Playback L', 'Playback R'],
  );
  assert.equal(state.strips[0].voice_chain, true);
  assert.ok(state.strips.slice(1).every((s) => s.voice_chain === false));
  assert.deepEqual(state.buses.map((b) => b.id), ['main', 'stream', 'monitor']);
  assert.deepEqual(state.scenes, []);
});

test('mock controller rejects unknown strip index and bus id', async () => {
  const controller = createMockMixerController();
  await assert.rejects(() => controller.setStripParam(99, 'fader_db', 0));
  await assert.rejects(() => controller.setBusParam('nope', 'fader_db', 0));
});

test('mock controller setStripParam clamps and persists across getState', async () => {
  const controller = createMockMixerController();
  await controller.setStripParam(1, 'fader_db', 999);
  const state = await controller.getState();
  assert.equal(state.strips[1].fader_db, 10);
});

test('mock controller setStripBool enforces voice_chain exclusivity', async () => {
  const controller = createMockMixerController();
  await controller.setStripBool(3, 'voice_chain', true);
  const state = await controller.getState();
  assert.equal(state.strips[0].voice_chain, false);
  assert.equal(state.strips[3].voice_chain, true);
});

test('mock controller renameStrip trims and caps at 24 characters', async () => {
  const controller = createMockMixerController();
  await controller.renameStrip(0, `  ${'x'.repeat(40)}  `);
  const state = await controller.getState();
  assert.equal(state.strips[0].name.length, 24);
});

test('mock controller renameStrip rejects an empty name', async () => {
  const controller = createMockMixerController();
  await assert.rejects(() => controller.renameStrip(0, '   '));
});

test('mock controller scenes: save, list, load and delete round-trip', async () => {
  const controller = createMockMixerController();
  await controller.setStripParam(1, 'fader_db', -12);
  await controller.saveScene('Worship');

  assert.deepEqual(await controller.listScenes(), ['Worship']);

  await controller.setStripParam(1, 'fader_db', 0);
  assert.equal((await controller.getState()).strips[1].fader_db, 0);

  await controller.loadScene('Worship');
  assert.equal((await controller.getState()).strips[1].fader_db, -12);

  await controller.deleteScene('Worship');
  assert.deepEqual(await controller.listScenes(), []);
});

test('mock controller saveScene rejects an invalid name', async () => {
  const controller = createMockMixerController();
  await assert.rejects(() => controller.saveScene('../evil'));
});

test('mock controller loadScene rejects an unknown scene', async () => {
  const controller = createMockMixerController();
  await assert.rejects(() => controller.loadScene('Nope'));
});

test('mock controller subscribeMeters emits animated data at ~20 Hz and respects mute', async () => {
  const controller = createMockMixerController();
  await controller.setStripBool(2, 'mute', true);

  const received = [];
  const sub = controller.subscribeMeters((payload) => received.push(payload));

  await new Promise((resolve) => setTimeout(resolve, 160));
  sub.dispose();

  assert.ok(received.length >= 2, `expected several ticks, got ${received.length}`);
  const last = received[received.length - 1];
  assert.equal(last.strips.length, 8);
  assert.equal(last.strips[2].post_db, -96, 'muted strip is silent post-fader');
  assert.equal(last.buses.length, 3);
  for (const bus of last.buses) {
    assert.ok(['main', 'stream', 'monitor'].includes(bus.id));
    assert.ok(Number.isFinite(bus.peak_l_db));
  }
});

test('mock controller subscribeMeters stops the timer once every listener disposes', async () => {
  const controller = createMockMixerController();
  let count = 0;
  const sub = controller.subscribeMeters(() => { count += 1; });
  await new Promise((resolve) => setTimeout(resolve, 80));
  sub.dispose();
  const countAfterDispose = count;
  await new Promise((resolve) => setTimeout(resolve, 80));
  assert.equal(count, countAfterDispose, 'no further ticks after the only subscriber disposes');
});

test('createCoalescer sends only the latest value per key on each flush', async () => {
  let flush = null;
  const coalesce = createCoalescer((fn) => { flush = fn; });
  const sent = [];
  const send = (key, v) => coalesce(key, async () => { sent.push([key, v]); return v; });

  const first = send('s0:fader_db', -10);
  const second = send('s0:fader_db', -5);
  const other = send('s1:pan', 0.5);
  flush();

  assert.equal(await first, undefined);
  assert.equal(await second, -5);
  assert.equal(await other, 0.5);
  assert.deepEqual(sent, [['s0:fader_db', -5], ['s1:pan', 0.5]]);
});

test('createCoalescer rejects when the surviving call fails and schedules again after a flush', async () => {
  const flushes = [];
  const coalesce = createCoalescer((fn) => { flushes.push(fn); });
  const failing = coalesce('k', async () => { throw new Error('engine said no'); });
  flushes[0]();
  await assert.rejects(failing, /engine said no/);

  const later = coalesce('k', async () => 'ok');
  assert.equal(flushes.length, 2);
  flushes[1]();
  assert.equal(await later, 'ok');
});

import test from 'node:test';
import assert from 'node:assert/strict';

import {
  EMPTY_OBS_STATUS,
  buildLinkRows,
  connectionState,
  createObsController,
  describeObsError,
  followObsStatus,
  lookupLink,
  normalizeSceneLink,
  obsPillText,
  parsePort,
  setLinkEntry,
} from './obsControl.js';

function recordingInvoke() {
  const calls = [];
  const invoke = async (command, payload) => {
    calls.push({ command, payload });
    return null;
  };
  return { calls, invoke };
}

test('createObsController wraps every command with the exact names/args', async () => {
  const { calls, invoke } = recordingInvoke();
  const obs = createObsController({ invoke, listen: async () => () => {} });

  await obs.getConfig();
  await obs.setConfig({ host: '127.0.0.1', port: 4455, password: 'pw', enabled: true });
  await obs.setConfig({
    host: 'h',
    port: 1,
    enabled: false,
    sceneLink: { enabled: true, studioToObs: { A: 'X' }, obsToStudio: {} },
  });
  await obs.getStatus();
  await obs.setScene('Pulpit Cam');
  await obs.startStream(true);
  await obs.stopStream(true);
  await obs.startRecord(true);
  await obs.stopRecord(true);

  assert.deepEqual(calls, [
    { command: 'obs_get_config', payload: undefined },
    {
      command: 'obs_set_config',
      payload: { host: '127.0.0.1', port: 4455, password: 'pw', enabled: true, sceneLink: null },
    },
    {
      command: 'obs_set_config',
      payload: {
        host: 'h',
        port: 1,
        password: null,
        enabled: false,
        sceneLink: { enabled: true, studioToObs: { A: 'X' }, obsToStudio: {} },
      },
    },
    { command: 'obs_status', payload: undefined },
    { command: 'obs_set_scene', payload: { name: 'Pulpit Cam' } },
    { command: 'obs_start_stream', payload: { confirm: true } },
    { command: 'obs_stop_stream', payload: { confirm: true } },
    { command: 'obs_start_record', payload: { confirm: true } },
    { command: 'obs_stop_record', payload: { confirm: true } },
  ]);
});

test('live-output calls send confirm=false unless explicitly true', async () => {
  const { calls, invoke } = recordingInvoke();
  const obs = createObsController({ invoke, listen: async () => () => {} });
  await obs.startStream();
  await obs.stopStream('yes');
  await obs.startRecord(1);
  await obs.stopRecord(false);
  assert.deepEqual(calls.map((c) => c.payload.confirm), [false, false, false, false]);
});

test('subscribeStatus listens on obs-status and disposes cleanly', async () => {
  const events = [];
  let unlistened = false;
  const listen = async (event, cb) => {
    events.push(event);
    listen.cb = cb;
    return () => { unlistened = true; };
  };
  const obs = createObsController({ invoke: async () => null, listen });
  const received = [];
  const sub = obs.subscribeStatus((payload) => received.push(payload));
  await sub.ready;
  listen.cb({ payload: { connected: true } });
  sub.dispose();

  assert.deepEqual(events, ['obs-status']);
  assert.deepEqual(received, [{ connected: true }]);
  assert.equal(unlistened, true);
});

test('connectionState maps the status to the dot', () => {
  assert.equal(connectionState(null), 'disconnected');
  assert.equal(connectionState(EMPTY_OBS_STATUS), 'disconnected');
  assert.equal(connectionState({ enabled: true, connecting: true }), 'connecting');
  assert.equal(connectionState({ enabled: true, connected: false }), 'connecting');
  assert.equal(connectionState({ enabled: true, connected: true }), 'connected');
});

test('obsPillText shows LIVE / REC only when connected', () => {
  assert.equal(obsPillText(null), null);
  assert.equal(obsPillText({ connected: false, streaming: true }), null);
  assert.equal(obsPillText({ connected: true }), 'OBS ●');
  assert.equal(obsPillText({ connected: true, streaming: true }), 'OBS ● LIVE');
  assert.equal(obsPillText({ connected: true, recording: true }), 'OBS ● REC');
  assert.equal(obsPillText({ connected: true, streaming: true, recording: true }), 'OBS ● LIVE / REC');
});

test('describeObsError passes backend strings through', () => {
  assert.equal(describeObsError('Wrong OBS password'), 'Wrong OBS password');
  assert.equal(describeObsError(new Error('boom')), 'boom');
  assert.equal(describeObsError(undefined), 'Something went wrong talking to OBS');
});

test('parsePort accepts 1..65535 only', () => {
  assert.equal(parsePort('4455'), 4455);
  assert.equal(parsePort(' 80 '), 80);
  assert.equal(parsePort('0'), null);
  assert.equal(parsePort('65536'), null);
  assert.equal(parsePort('44a5'), null);
  assert.equal(parsePort(''), null);
});

test('setLinkEntry is immutable and removes on empty', () => {
  const original = { Worship: 'Band' };
  const added = setLinkEntry(original, ' Sermon ', ' Pulpit ');
  assert.deepEqual(added, { Worship: 'Band', Sermon: 'Pulpit' });
  assert.deepEqual(original, { Worship: 'Band' });
  assert.deepEqual(setLinkEntry(added, 'Worship', ''), { Sermon: 'Pulpit' });
  assert.deepEqual(setLinkEntry(undefined, '', 'x'), {});
  assert.deepEqual(setLinkEntry({ ' worship ': 'A' }, 'Worship', 'B'), { Worship: 'B' });
});

test('lookupLink and buildLinkRows match case-insensitively on trimmed names', () => {
  const map = { ' worship ': 'Band Wide ' };
  assert.equal(lookupLink(map, 'WORSHIP'), 'Band Wide');
  assert.equal(lookupLink(map, 'Sermon'), '');
  assert.deepEqual(buildLinkRows(['Worship', 'Sermon'], map), [
    { scene: 'Worship', target: 'Band Wide' },
    { scene: 'Sermon', target: '' },
  ]);
});

test('normalizeSceneLink fills defaults', () => {
  assert.deepEqual(normalizeSceneLink(undefined), { enabled: false, studioToObs: {}, obsToStudio: {} });
  assert.deepEqual(normalizeSceneLink({ enabled: 1, studioToObs: { A: 'B' } }), {
    enabled: true,
    studioToObs: { A: 'B' },
    obsToStudio: {},
  });
});

function fakeStatusController() {
  let emit = null;
  let resolveReady;
  let resolveSnapshot;
  const calls = [];
  const controller = {
    subscribeStatus(cb) {
      emit = cb;
      return { ready: new Promise((r) => { resolveReady = r; }), dispose() { emit = null; } };
    },
    getStatus() {
      calls.push('getStatus');
      return new Promise((r) => { resolveSnapshot = r; });
    },
  };
  return {
    controller,
    calls,
    emit: (s) => emit && emit(s),
    ready: () => resolveReady(),
    snapshot: (s) => resolveSnapshot(s),
  };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

test('followObsStatus reads the snapshot only once the listener is ready', async () => {
  const f = fakeStatusController();
  const seen = [];
  followObsStatus(f.controller, (s) => seen.push(s.state));
  await tick();
  assert.deepEqual(f.calls, [], 'no snapshot before the listener is registered');
  f.ready();
  await tick();
  assert.deepEqual(f.calls, ['getStatus']);
  f.snapshot({ state: 'connected' });
  await tick();
  assert.deepEqual(seen, ['connected']);
});

test('followObsStatus ignores a snapshot that lands after a newer event', async () => {
  const f = fakeStatusController();
  const seen = [];
  const dispose = followObsStatus(f.controller, (s) => seen.push(s.state));
  f.ready();
  await tick();
  f.emit({ state: 'connected' });
  f.snapshot({ state: 'disconnected' });
  await tick();
  assert.deepEqual(seen, ['connected']);
  dispose();
  f.emit({ state: 'error' });
  assert.deepEqual(seen, ['connected']);
});

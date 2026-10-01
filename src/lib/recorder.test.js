import test from 'node:test';
import assert from 'node:assert/strict';

import {
  EMPTY_RECORDER_STATUS,
  RECORDER_STATUS_EVENT,
  armedIndices,
  createRecorderController,
  describeRecorderError,
  followRecorderStatus,
  formatElapsed,
  normalizeRecorderConfig,
  normalizeRecorderStatus,
  recorderHealth,
  recorderPillText,
  toggleArmedStrip,
} from './recorder.js';

function recordingInvoke(result = null) {
  const calls = [];
  const invoke = async (command, payload) => {
    calls.push({ command, payload });
    return result;
  };
  return { calls, invoke };
}

test('controller wraps every command with the exact names and camelCase args', async () => {
  const { calls, invoke } = recordingInvoke();
  const rec = createRecorderController({
    invoke,
    listen: async () => () => {},
    utcOffsetMinutes: () => 60,
  });

  await rec.getStatus();
  await rec.getConfig();
  await rec.setConfig({ folder: 'D:/Rec', includeMain: true, armedStrips: [0, 2] });
  await rec.setConfig({});
  await rec.start({ title: 'Sunday AM' });
  await rec.start();
  await rec.stop();
  await rec.addMarker('Sermon');
  await rec.openFolder();

  assert.deepEqual(calls, [
    { command: 'recorder_status', payload: undefined },
    { command: 'recorder_get_config', payload: undefined },
    { command: 'recorder_set_config', payload: { folder: 'D:/Rec', includeMain: true, armedStrips: [0, 2] } },
    { command: 'recorder_set_config', payload: { folder: null, includeMain: false, armedStrips: null } },
    { command: 'recorder_start', payload: { title: 'Sunday AM', utcOffsetMinutes: 60 } },
    { command: 'recorder_start', payload: { title: null, utcOffsetMinutes: 60 } },
    { command: 'recorder_stop', payload: undefined },
    { command: 'recorder_add_marker', payload: { label: 'Sermon' } },
    { command: 'recorder_open_folder', payload: undefined },
  ]);
});

test('blank titles and folders are sent as null', async () => {
  const { calls, invoke } = recordingInvoke();
  const rec = createRecorderController({ invoke, listen: async () => () => {}, utcOffsetMinutes: () => 0 });
  await rec.start({ title: '   ' });
  await rec.setConfig({ folder: '  ', includeMain: false, armedStrips: null });
  assert.equal(calls[0].payload.title, null);
  assert.equal(calls[1].payload.folder, null);
});

test('the default UTC offset is the local one, east positive', async () => {
  const { calls, invoke } = recordingInvoke();
  const rec = createRecorderController({ invoke, listen: async () => () => {} });
  await rec.start();
  assert.equal(calls[0].payload.utcOffsetMinutes, -new Date().getTimezoneOffset());
});

test('subscribeStatus listens to recorder-status', async () => {
  const seen = [];
  let handler = null;
  const listen = async (event, cb) => {
    seen.push(event);
    handler = cb;
    return () => {};
  };
  const rec = createRecorderController({ invoke: async () => null, listen });
  const got = [];
  const sub = rec.subscribeStatus((payload) => got.push(payload));
  await sub.ready;
  handler({ payload: { state: 'recording' } });
  assert.deepEqual(seen, [RECORDER_STATUS_EVENT]);
  assert.deepEqual(got, [{ state: 'recording' }]);
  sub.dispose();
});

test('followRecorderStatus ignores a snapshot older than an event', async () => {
  let handler = null;
  const controller = {
    subscribeStatus(cb) {
      handler = cb;
      return { ready: Promise.resolve(), dispose() {} };
    },
    getStatus: async () => {
      handler({ state: 'recording', elapsedSeconds: 3 });
      return { state: 'idle' };
    },
  };
  const got = [];
  followRecorderStatus(controller, (s) => got.push(s.state));
  await new Promise((resolve) => setTimeout(resolve, 0));
  assert.deepEqual(got, ['recording']);
});

test('formatElapsed is H:MM:SS', () => {
  assert.equal(formatElapsed(0), '0:00:00');
  assert.equal(formatElapsed(5.9), '0:00:05');
  assert.equal(formatElapsed(65), '0:01:05');
  assert.equal(formatElapsed(3600 * 5 + 61), '5:01:01');
  assert.equal(formatElapsed(-3), '0:00:00');
  assert.equal(formatElapsed(Number.NaN), '0:00:00');
});

test('normalizeRecorderStatus tolerates missing fields', () => {
  assert.deepEqual(normalizeRecorderStatus(null), EMPTY_RECORDER_STATUS);
  const status = normalizeRecorderStatus({
    state: 'recording',
    engineRunning: true,
    elapsedSeconds: 12,
    folder: 'D:/Rec/2026-10-01 1030',
    tracks: ['Pastor', 7, 'Stream Mix'],
    droppedFrames: 4,
    markers: 2,
    lastError: null,
    lastSummary: null,
  });
  assert.equal(status.recording, true);
  assert.deepEqual(status.tracks, ['Pastor', 'Stream Mix']);
  assert.equal(status.droppedFrames, 4);
  assert.equal(normalizeRecorderStatus({ state: 'weird' }).state, 'unavailable');
});

test('normalizeRecorderConfig keeps null armed strips as "all"', () => {
  assert.deepEqual(normalizeRecorderConfig(null), {
    folder: null,
    defaultFolder: null,
    includeMain: false,
    armedStrips: null,
  });
  assert.deepEqual(
    normalizeRecorderConfig({ folder: 'D:/x', defaultFolder: 'C:/d', includeMain: 1, armedStrips: [2, 'a', 0] }),
    { folder: 'D:/x', defaultFolder: 'C:/d', includeMain: true, armedStrips: [0, 2] },
  );
});

test('armed strips: null arms every strip; toggling builds an explicit list', () => {
  const strips = [0, 1, 2];
  assert.deepEqual(armedIndices(null, strips), [0, 1, 2]);
  assert.deepEqual(armedIndices([2, 9], strips), [2]);
  assert.deepEqual(toggleArmedStrip(null, 1, strips), [0, 2]);
  assert.deepEqual(toggleArmedStrip([0, 2], 1, strips), null, 'all armed again → null');
  assert.deepEqual(toggleArmedStrip([0], 0, strips), []);
});

test('pill text only while recording', () => {
  assert.equal(recorderPillText(EMPTY_RECORDER_STATUS), null);
  assert.equal(
    recorderPillText(normalizeRecorderStatus({ state: 'recording', elapsedSeconds: 61 })),
    'REC 0:01:01',
  );
});

test('health line reports drops and the last error', () => {
  assert.deepEqual(recorderHealth(normalizeRecorderStatus({ state: 'recording' })), {
    level: 'ok',
    text: 'No dropped frames',
  });
  assert.equal(recorderHealth(normalizeRecorderStatus({ state: 'recording', droppedFrames: 12 })).level, 'warn');
  const failed = recorderHealth(normalizeRecorderStatus({ state: 'idle', lastError: 'disk full' }));
  assert.deepEqual(failed, { level: 'error', text: 'Recording stopped: disk full' });
});

test('describeRecorderError handles strings and errors', () => {
  assert.equal(describeRecorderError('nope'), 'nope');
  assert.equal(describeRecorderError(new Error('bad')), 'bad');
  assert.equal(describeRecorderError(null), 'The recorder did not respond.');
});

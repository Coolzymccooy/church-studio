import test from 'node:test';
import assert from 'node:assert/strict';

import {
  EXPORT_PROGRESS_EVENT,
  createExportController,
  defaultExportTrack,
  describeExportError,
  editorExportName,
  encodeWavPcm16,
  exportFilter,
  exportTracks,
  formatExportPercent,
  normalizeExportProgress,
} from './audioExport.js';

function recordingInvoke(result = null) {
  const calls = [];
  const invoke = async (command, payload, options) => {
    calls.push({ command, payload, options });
    return result;
  };
  return { calls, invoke };
}

test('exportTrack sends the track, format and bitrate', async () => {
  const { calls, invoke } = recordingInvoke({ file: 'Stream Mix.mp3' });
  const controller = createExportController({ invoke, listen: async () => () => {} });
  const result = await controller.exportTrack({ track: 'Stream Mix.wav', format: 'mp3' });
  assert.equal(result.file, 'Stream Mix.mp3');
  assert.deepEqual(calls, [{
    command: 'recorder_export',
    payload: { track: 'Stream Mix.wav', format: 'mp3', bitrateKbps: null },
    options: undefined,
  }]);
});

test('exportEditorAudio sends raw bytes and an encoded destination header', async () => {
  const { calls, invoke } = recordingInvoke({});
  const controller = createExportController({ invoke, listen: async () => () => {} });
  const bytes = new Uint8Array([1, 2, 3]);
  await controller.exportEditorAudio({ wavBytes: bytes, format: 'm4a', dest: '/Users/a/Sérmon mix.m4a' });
  assert.equal(calls[0].command, 'editor_export');
  assert.equal(calls[0].payload, bytes);
  assert.deepEqual(calls[0].options.headers, {
    'x-export-format': 'm4a',
    'x-export-dest': '%2FUsers%2Fa%2FS%C3%A9rmon%20mix.m4a',
  });
});

test('subscribeProgress listens to export-progress payloads', async () => {
  const seen = [];
  let handler = null;
  const listen = async (name, cb) => {
    assert.equal(name, EXPORT_PROGRESS_EVENT);
    handler = cb;
    return () => {};
  };
  const controller = createExportController({ invoke: async () => null, listen });
  const sub = controller.subscribeProgress((p) => seen.push(p));
  await sub.ready;
  handler({ payload: { fraction: 0.5 } });
  sub.dispose();
  handler({ payload: { fraction: 0.6 } });
  assert.deepEqual(seen, [{ fraction: 0.5 }]);
});

test('exportTracks keeps valid tracks and the Stream mix is the default', () => {
  const tracks = exportTracks({
    tracks: [
      { name: 'Pastor', file: '01 Pastor.wav', kind: 'strip' },
      null,
      { name: 'Bad', file: '' },
      { name: 'Main Mix', file: 'Main Mix.wav', kind: 'bus' },
      { name: 'Stream Mix', file: 'Stream Mix.wav', kind: 'bus' },
    ],
  });
  assert.equal(tracks.length, 3);
  assert.equal(defaultExportTrack(tracks), 'Stream Mix.wav');
  assert.equal(defaultExportTrack(tracks.slice(0, 2)), 'Main Mix.wav');
  assert.equal(defaultExportTrack(tracks.slice(0, 1)), '01 Pastor.wav');
  assert.equal(defaultExportTrack([]), null);
  assert.deepEqual(exportTracks(null), []);
  assert.deepEqual(exportTracks({ tracks: 'x' }), []);
});

test('progress is normalised and printed as a whole percent', () => {
  assert.deepEqual(normalizeExportProgress({ file: 'a.mp3', format: 'mp3', fraction: 1.4 }), {
    file: 'a.mp3', format: 'mp3', fraction: 1,
  });
  assert.deepEqual(normalizeExportProgress(null), { file: '', format: null, fraction: 0 });
  assert.equal(formatExportPercent(0.428), '42%');
  assert.equal(formatExportPercent(NaN), '0%');
  assert.equal(formatExportPercent(1), '100%');
});

test('errors, names and filters', () => {
  assert.equal(describeExportError('disk full'), 'disk full');
  assert.equal(describeExportError(new Error('x')), 'x');
  assert.equal(describeExportError(undefined), 'The export did not finish.');
  assert.equal(editorExportName('mp3', 42), 'TIWATON_edited_42.mp3');
  assert.deepEqual(exportFilter('m4a').extensions, ['m4a']);
  assert.deepEqual(exportFilter('mp3').extensions, ['mp3']);
});

test('encodeWavPcm16 writes a 16-bit PCM WAV, interleaved and clamped', () => {
  const left = new Float32Array([0.5, -1.5]);
  const right = new Float32Array([0, 1]);
  const view = new DataView(encodeWavPcm16([left, right], 48000));
  const text = (o, n) => String.fromCharCode(...new Uint8Array(view.buffer, o, n));
  assert.equal(text(0, 4), 'RIFF');
  assert.equal(text(8, 4), 'WAVE');
  assert.equal(view.getUint16(20, true), 1);
  assert.equal(view.getUint16(22, true), 2);
  assert.equal(view.getUint32(24, true), 48000);
  assert.equal(view.getUint32(40, true), 8);
  assert.equal(view.byteLength, 52);
  assert.equal(view.getInt16(44, true), 16383);
  assert.equal(view.getInt16(46, true), 0);
  assert.equal(view.getInt16(48, true), -32768);
  assert.equal(view.getInt16(50, true), 32767);
});

import test from 'node:test';
import assert from 'node:assert/strict';

import { createPreviewController, normalizePreviewStatus, previewKeyLabel } from './recordPreview.js';

test('the controller calls the play commands with the right arguments', async () => {
  const calls = [];
  const invoke = async (cmd, args) => { calls.push([cmd, args]); return { playing: true, positionSeconds: 0, durationSeconds: 3 }; };
  const c = createPreviewController({ invoke });
  await c.play();
  await c.play('01 Pastor.wav');
  await c.stop();
  await c.status();
  assert.deepEqual(calls, [
    ['recorder_play', { track: null }],
    ['recorder_play', { track: '01 Pastor.wav' }],
    ['recorder_play_stop', undefined],
    ['recorder_play_status', undefined],
  ]);
});

test('status is cleaned up from whatever the engine sends', () => {
  assert.deepEqual(normalizePreviewStatus(null), { playing: false, positionSeconds: 0, durationSeconds: 0 });
  assert.deepEqual(
    normalizePreviewStatus({ playing: true, positionSeconds: 12.4, durationSeconds: 'x' }),
    { playing: true, positionSeconds: 12.4, durationSeconds: 0 },
  );
});

test('the key says play, or stop with the elapsed time', () => {
  assert.equal(previewKeyLabel({ playing: false, positionSeconds: 0 }), 'play');
  assert.equal(previewKeyLabel({ playing: true, positionSeconds: 72.9 }), 'stop 1:12');
  assert.equal(previewKeyLabel({ playing: true, positionSeconds: 5 }), 'stop 0:05');
});

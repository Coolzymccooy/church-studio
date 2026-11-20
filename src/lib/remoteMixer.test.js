import assert from 'node:assert/strict';
import test from 'node:test';
import { changeBody, dbLabel } from '../../src-tauri/src/remote_mixer/web/protocol.js';

test('remote clients send typed changes with the latest layout signature', () => {
  const target = { type: 'strip', index: 3 };
  assert.deepEqual(changeBody('abc', target, 'level', '-12.5'), {
    layout: 'abc', target, kind: 'level', db: -12.5,
  });
  const main = { type: 'bus', id: 'main' };
  assert.deepEqual(changeBody('123', main, 'mute', false), {
    layout: '123', target: main, kind: 'mute', muted: false,
  });
  assert.equal(dbLabel(-90), 'Off');
  assert.equal(dbLabel(-12.5), '-12.5 dB');
});

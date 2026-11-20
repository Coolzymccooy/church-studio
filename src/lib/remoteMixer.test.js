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

test('the pairing URL keeps the code in a fragment', async () => {
  const { pairCodeFromHash, pairUrl } = await import('../../src-tauri/src/remote_mixer/web/protocol.js');
  const url = new URL(pairUrl('https://desk.example.test:4475', '012345'));
  assert.equal(url.search, '');
  assert.equal(pairCodeFromHash(url.hash), '012345');
  assert.equal(pairCodeFromHash('#code=12345x'), '');
});

test('incoming desk changes preserve only the faders held by the local operator', async () => {
  const { keepHeldFaders } = await import('./heldFaders.js');
  const state = { strips: [{ index: 0, fader_db: 0 }, { index: 1, fader_db: -8 }], buses: [{ id: 'main', fader_db: -10 }] };
  const next = { strips: [{ index: 0, fader_db: -20, mute: true }, { index: 1, fader_db: -16 }], buses: [{ id: 'main', fader_db: -30 }] };
  const result = keepHeldFaders(state, next, new Set(['strip:0', 'bus:main']));
  assert.deepEqual(result.strips, [{ index: 0, fader_db: 0, mute: true }, next.strips[1]]);
  assert.equal(result.buses[0].fader_db, -10);
  assert.equal(keepHeldFaders(result, next, new Set()), next);
});

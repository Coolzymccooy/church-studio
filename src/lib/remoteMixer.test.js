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

test('a busy snapshot does not drop the newest fader value or other queued changes', async () => {
  const { drainControlQueue } = await import('../../src-tauri/src/remote_mixer/web/protocol.js');
  const queue = new Map();
  const active = new Set();
  const writes = [];
  const applied = [];
  const failures = [];
  const fader = 'strip:0:level';
  const mute = 'strip:1:mute';
  const enqueue = (id, value) => queue.set(id, { change: value, retryUntil: 2_000 });
  let snapshotHoldingLock = true;
  let now = 0;
  enqueue(fader, -3);
  const post = async (value) => {
    writes.push(value);
    if (snapshotHoldingLock) {
      // The SSE reader just acquired the engine lock. A newer fader position
      // and a different control arrive while this write is in flight.
      enqueue(fader, -24);
      enqueue(mute, true);
      return 503;
    }
    applied.push(value);
    return 204;
  };
  await drainControlQueue({
    queue, active, post, now: () => now,
    respond: (status) => { if (status !== 204) failures.push(status); },
    wait: async (ms) => {
      assert.equal(active.has(fader), true);
      now += ms;
      snapshotHoldingLock = false;
    },
  });
  assert.deepEqual(writes, [-3, -24, true]);
  assert.deepEqual(applied, [-24, true]);
  assert.deepEqual(failures, []);
  assert.equal(active.size, 0);
  assert.equal(queue.size, 0);
});

test('busy and throttled writes retry briefly and report failure when the budget runs out', async () => {
  const { drainControlQueue } = await import('../../src-tauri/src/remote_mixer/web/protocol.js');
  for (const status of [429, 503]) {
    const queue = new Map([['strip:0:level', { change: -6, retryUntil: 80 }]]);
    const active = new Set();
    const outcomes = [];
    let now = 0;
    let writes = 0;
    await drainControlQueue({
      queue, active, now: () => now,
      post: async () => { writes++; return status; },
      wait: async (ms) => { now += ms; },
      respond: (result) => outcomes.push(result),
    });
    assert.equal(writes, 2);
    assert.deepEqual(outcomes, [status]);
    assert.equal(queue.size, 0);
    assert.equal(active.size, 0);
  }
});

test('a final fader value retries on busy until it succeeds', async () => {
  const { drainControlQueue } = await import('../../src-tauri/src/remote_mixer/web/protocol.js');
  const queue = new Map([['strip:0:level', { change: -21.5, retryUntil: 2_000 }]]);
  const active = new Set();
  const writes = [];
  const outcomes = [];
  await drainControlQueue({
    queue, active, now: () => 0, wait: async () => {},
    post: async (value) => {
      writes.push(value);
      return writes.length === 1 ? 503 : 204;
    },
    respond: (status) => outcomes.push(status),
  });
  assert.deepEqual(writes, [-21.5, -21.5]);
  assert.deepEqual(outcomes, [204]);
});

test('an expired retry is discarded before it can change a fader', async () => {
  const { drainControlQueue } = await import('../../src-tauri/src/remote_mixer/web/protocol.js');
  const queue = new Map([['strip:0:level', { change: -21.5, retryUntil: 2_000 }]]);
  let now = 0;
  let writes = 0;
  const outcomes = [];
  await drainControlQueue({
    queue, active: new Set(), now: () => now,
    post: async () => { writes++; return 503; },
    wait: async () => { now = 2_100; },
    respond: (status) => outcomes.push(status),
  });
  assert.equal(writes, 1);
  assert.deepEqual(outcomes, [503]);
});

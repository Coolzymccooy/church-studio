import test from 'node:test';
import assert from 'node:assert/strict';

import { createCachedLoader } from './rnnoise.js';

test('createCachedLoader only invokes the underlying loader once', async () => {
  let calls = 0;
  const load = createCachedLoader(async () => {
    calls += 1;
    return { value: 'wasm-bytes' };
  });

  const a = await load();
  const b = await load();

  assert.equal(calls, 1);
  assert.equal(a, b);
  assert.deepEqual(a, { value: 'wasm-bytes' });
});

test('createCachedLoader shares a single in-flight promise across concurrent callers', async () => {
  let calls = 0;
  let resolveLoad;
  const load = createCachedLoader(
    () =>
      new Promise((resolve) => {
        calls += 1;
        resolveLoad = resolve;
      }),
  );

  const p1 = load();
  const p2 = load();

  assert.equal(calls, 1, 'loader should only start once while a load is in flight');

  resolveLoad('done');
  const [r1, r2] = await Promise.all([p1, p2]);
  assert.equal(r1, 'done');
  assert.equal(r2, 'done');
});

test('createCachedLoader does not cache a failed load, allowing retry', async () => {
  let calls = 0;
  const load = createCachedLoader(async () => {
    calls += 1;
    if (calls === 1) throw new Error('network error');
    return 'ok';
  });

  await assert.rejects(load(), /network error/);
  assert.equal(calls, 1);

  const result = await load();
  assert.equal(result, 'ok');
  assert.equal(calls, 2);

  // Now that it has succeeded, further calls are cached.
  const result2 = await load();
  assert.equal(result2, 'ok');
  assert.equal(calls, 2);
});

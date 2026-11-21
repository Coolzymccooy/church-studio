import test from 'node:test';
import assert from 'node:assert/strict';

import { copyText } from './copyText.js';

function fakeDoc({ execResult = true } = {}) {
  const calls = [];
  const area = {
    value: '', style: {}, attrs: {},
    setAttribute(k, v) { this.attrs[k] = v; },
    select() { calls.push(['select', this.value]); },
  };
  return {
    calls,
    area,
    body: { appendChild: (el) => calls.push(['append', el]), removeChild: (el) => calls.push(['remove', el]) },
    createElement: () => area,
    execCommand: (cmd) => { calls.push(['exec', cmd]); return execResult; },
  };
}

test('uses the async clipboard when it works', async () => {
  let written = null;
  const nav = { clipboard: { writeText: async (t) => { written = t; } } };
  assert.equal(await copyText('abc', { nav, doc: fakeDoc() }), true);
  assert.equal(written, 'abc');
});

test('falls back to a hidden text area when the async clipboard is missing or refuses', async () => {
  for (const nav of [{}, { clipboard: { writeText: async () => { throw new Error('NotAllowed'); } } }]) {
    const doc = fakeDoc();
    assert.equal(await copyText('tok', { nav, doc }), true);
    assert.deepEqual(doc.calls.map((c) => c[0]), ['append', 'select', 'exec', 'remove']);
    assert.equal(doc.calls[1][1], 'tok');
    assert.equal(doc.area.attrs.readonly, '');
  }
});

test('reports failure when nothing can copy, and never throws', async () => {
  assert.equal(await copyText('x', { nav: {}, doc: fakeDoc({ execResult: false }) }), false);
  const broken = { ...fakeDoc(), execCommand: () => { throw new Error('boom'); } };
  assert.equal(await copyText('x', { nav: {}, doc: broken }), false);
  assert.equal(await copyText('', { nav: {}, doc: fakeDoc() }), false);
});

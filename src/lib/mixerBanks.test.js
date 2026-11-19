import test from 'node:test';
import assert from 'node:assert/strict';
import { bankStrips, pageOf, pickBank, stripBanks } from './mixerBanks.js';

test('one bank ("all") when the strips fit in eight', () => {
  assert.deepEqual(stripBanks(6, 0).map((b) => b.id), ['all']);
  assert.deepEqual(stripBanks(8, 2).map((b) => b.id), ['all']);
});

test('input banks of eight, then ndi', () => {
  const banks = stripBanks(20, 4);
  assert.deepEqual(banks.map((b) => b.label), ['all', '1–8', '9–16', 'ndi']);
  assert.deepEqual(pickBank(banks, 'in-8'), { id: 'in-8', label: '9–16', from: 8, to: 16 });
  assert.deepEqual(pickBank(banks, 'ndi'), { id: 'ndi', label: 'ndi', from: 16, to: 20 });
  assert.equal(pickBank(banks, 'in-24').id, 'all');
});

test('bankStrips filters by index range', () => {
  const strips = Array.from({ length: 12 }, (_, index) => ({ index }));
  const [, first, second] = stripBanks(12, 0);
  assert.equal(bankStrips(strips, first).length, 8);
  assert.deepEqual(bankStrips(strips, second).map((s) => s.index), [8, 9, 10, 11]);
});

test('pageOf clamps the page and reports neighbours', () => {
  const items = ['a', 'b', 'c', 'd', 'e', 'f', 'g'];
  assert.deepEqual(pageOf(items, 0, 6), { items: items.slice(0, 6), page: 0, hasPrev: false, hasNext: true, start: 0 });
  assert.deepEqual(pageOf(items, 9, 6), { items: ['g'], page: 1, hasPrev: true, hasNext: false, start: 6 });
  assert.deepEqual(pageOf([], 0, 6), { items: [], page: 0, hasPrev: false, hasNext: false, start: 0 });
});

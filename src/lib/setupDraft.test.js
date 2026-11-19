import test from 'node:test';
import assert from 'node:assert/strict';
import { clampPanel, draftChanges, draftFrom, isDirty, readPanelPos, rowStatus } from './setupDraft.js';

const saved = { inputId: 'mic', outputId: 'jabra', broadcastBus: 'Not set', mainOutputId: null, other: 1 };

test('draftFrom copies only the setup keys and is a new object', () => {
  const d = draftFrom(saved);
  assert.deepEqual(d, { inputId: 'mic', outputId: 'jabra', broadcastBus: 'Not set', mainOutputId: null });
  assert.notEqual(d, saved);
});

test('draftChanges returns only what changed; isDirty follows it', () => {
  const draft = { ...draftFrom(saved), outputId: 'speakers' };
  assert.deepEqual(draftChanges(saved, draft), { outputId: 'speakers' });
  assert.equal(isDirty(saved, draft), true);
  assert.equal(isDirty(saved, draftFrom(saved)), false);
});

test('clampPanel keeps the panel inside the viewport', () => {
  const size = { w: 360, h: 500 };
  const vp = { w: 1440, h: 900 };
  assert.deepEqual(clampPanel({ x: -50, y: -10 }, size, vp), { x: 8, y: 8 });
  assert.deepEqual(clampPanel({ x: 5000, y: 5000 }, size, vp), { x: 1440 - 360 - 8, y: 900 - 500 - 8 });
  assert.deepEqual(clampPanel({ x: 100.4, y: 200.6 }, size, vp), { x: 100, y: 201 });
  assert.deepEqual(clampPanel({ x: 100, y: 100 }, { w: 2000, h: 2000 }, vp), { x: 8, y: 8 });
});

test('readPanelPos rejects malformed values', () => {
  assert.deepEqual(readPanelPos('{"x":10,"y":20}'), { x: 10, y: 20 });
  assert.equal(readPanelPos('nope'), null);
  assert.equal(readPanelPos(null), null);
  assert.equal(readPanelPos('{"x":"a","y":2}'), null);
});

test('rowStatus: unsaved, not set, on, idle', () => {
  const draft = { ...draftFrom(saved), inputId: 'other' };
  assert.equal(rowStatus({ saved, draft, key: 'inputId', running: true }), 'unsaved');
  assert.equal(rowStatus({ saved, draft, key: 'broadcastBus', running: true }), 'not set');
  assert.equal(rowStatus({ saved, draft, key: 'mainOutputId', running: true }), 'not set');
  assert.equal(rowStatus({ saved, draft, key: 'outputId', running: true }), 'on');
  assert.equal(rowStatus({ saved, draft, key: 'outputId', running: false }), 'idle');
});

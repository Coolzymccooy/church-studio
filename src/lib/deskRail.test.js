import test from 'node:test';
import assert from 'node:assert/strict';
import { alertLamp, deskAlerts, popPosition } from './deskRail.js';

test('no alerts when all is well', () => {
  assert.deepEqual(deskAlerts({ ndi: { checked: true, available: true }, broadcastSet: true, isLive: true }), []);
  assert.equal(alertLamp([]), 'off');
});

test('alerts are ordered by severity and carry actions', () => {
  const alerts = deskAlerts({
    engineError: 'device lost', ndi: { checked: true, available: false }, broadcastSet: false,
    isLive: true, droppedSamples: 12, cpuPct: 95, lufsST: -9, target: -14,
  });
  assert.deepEqual(alerts.map((a) => a.id), ['engine', 'loud', 'drops', 'cpu', 'broadcast', 'ndi']);
  assert.equal(alerts.find((a) => a.id === 'ndi').action, 'ndi-tools');
  assert.equal(alerts.find((a) => a.id === 'broadcast').action, 'setup');
  assert.equal(alertLamp(alerts), 'r');
  assert.equal(alertLamp(alerts.filter((a) => a.tone === 'a')), 'a');
});

test('live-only alerts stay quiet while the engine is off; NDI waits for its check', () => {
  const alerts = deskAlerts({ isLive: false, droppedSamples: 5, cpuPct: 99, lufsST: 0, target: -14, ndi: { checked: false, available: false } });
  assert.deepEqual(alerts, []);
});

test('popPosition places beside the key and stays on screen', () => {
  const vp = { w: 1440, h: 900 };
  const size = { w: 300, h: 400 };
  assert.deepEqual(popPosition({ left: 1380, right: 1432, top: 100 }, size, vp, 'right'), { top: 100, left: 1072 });
  assert.deepEqual(popPosition({ left: 8, right: 60, top: 100 }, size, vp, 'left'), { top: 100, left: 68 });
  assert.equal(popPosition({ left: 1380, right: 1432, top: 800 }, size, vp, 'right').top, 900 - 400 - 8);
  assert.equal(popPosition({ left: 8, right: 60, top: -50 }, size, vp, 'left').top, 8);
});

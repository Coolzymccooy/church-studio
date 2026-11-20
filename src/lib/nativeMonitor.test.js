import test from 'node:test';
import assert from 'node:assert/strict';
import { MONITOR_MUTE_DB, monitorGainDb, readMonitorPref, setNativeMonitor } from './nativeMonitor.js';

test('monitor on is unity, off is muted', () => {
  assert.equal(monitorGainDb(true), 0);
  assert.equal(monitorGainDb(false), MONITOR_MUTE_DB);
  assert.ok(10 ** (MONITOR_MUTE_DB / 20) < 1e-5);
});

test('setNativeMonitor sends monitor_gain_db to the engine', async () => {
  const calls = [];
  await setNativeMonitor(async (cmd, args) => { calls.push([cmd, args]); }, false);
  assert.deepEqual(calls, [['set_param', { key: 'monitor_gain_db', value: MONITOR_MUTE_DB }]]);
});

test('readMonitorPref', () => {
  assert.equal(readMonitorPref('on'), true);
  assert.equal(readMonitorPref('off', true), false);
  assert.equal(readMonitorPref(null, true), true);
  assert.equal(readMonitorPref('maybe'), false);
});

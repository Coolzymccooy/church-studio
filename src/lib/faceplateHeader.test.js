import test from 'node:test';
import assert from 'node:assert/strict';

import { engineState, headerReadout, obsLamp, recLamp, gateLamp, formatClock } from './faceplateHeader.js';

const EMPTY = { sampleRate: 0, bufferSize: 0, latencyMs: null, callbackAvgMs: null, callbackPeakMs: null, cpuLoadPct: null, droppedOutputSamples: 0 };

test('engine state word and lamp follow live and file playback', () => {
  assert.deepEqual(engineState(true, false), { word: 'running', tone: 'g' });
  assert.deepEqual(engineState(false, true), { word: 'file playback', tone: 'g' });
  assert.deepEqual(engineState(false, false), { word: 'standby', tone: 'off' });
});

test('readout shows dashes before the engine reports', () => {
  assert.deepEqual(headerReadout(EMPTY).map((r) => r.value), ['--', '--', '--', '--', '--', '0']);
});

test('readout formats live stats and flags trouble', () => {
  const r = headerReadout({ ...EMPTY, sampleRate: 48000, bufferSize: 256, latencyMs: 5.333, cpuLoadPct: 71.2, callbackAvgMs: 0.84, callbackPeakMs: 2.5, droppedOutputSamples: 12 });
  assert.deepEqual(r.map((x) => [x.key, x.value, x.warn]), [
    ['sr', '48.0k', false], ['buf', '256', false], ['lat', '5.3 ms', false],
    ['cpu', '71%', true], ['cb', '0.84 / 2.50', false], ['drop', '12', true],
  ]);
});

test('cpu at 70% is not a warning yet; a missing callback peak still shows the average', () => {
  const r = headerReadout({ ...EMPTY, cpuLoadPct: 70, callbackAvgMs: 1 });
  assert.equal(r.find((x) => x.key === 'cpu').warn, false);
  assert.equal(r.find((x) => x.key === 'cb').value, '1.00');
});

test('obs lamp: off, connected, live, recording', () => {
  assert.deepEqual(obsLamp({ connected: false }), { word: 'obs', tone: 'off' });
  assert.deepEqual(obsLamp({ connected: true }), { word: 'obs', tone: 'g' });
  assert.deepEqual(obsLamp({ connected: true, streaming: true }), { word: 'obs live', tone: 'g' });
  assert.deepEqual(obsLamp({ connected: true, recording: true }), { word: 'obs rec', tone: 'r' });
  assert.deepEqual(obsLamp({ connected: true, streaming: true, recording: true }), { word: 'obs live · rec', tone: 'r' });
});

test('rec lamp: off, recording with time, dropped frames, error', () => {
  assert.deepEqual(recLamp({ recording: false }), { word: 'rec', tone: 'off' });
  assert.deepEqual(recLamp({ recording: true, elapsedSeconds: 75 }), { word: 'rec 0:01:15', tone: 'r' });
  assert.deepEqual(recLamp({ recording: true, elapsedSeconds: 5, droppedFrames: 3 }), { word: 'rec 0:00:05 · 3 dropped', tone: 'r' });
  assert.deepEqual(recLamp({ recording: false, lastError: 'disk full' }), { word: 'rec failed', tone: 'r' });
});

test('gate lamp only lights while running', () => {
  assert.deepEqual(gateLamp(false, true), { word: 'gate', tone: 'off' });
  assert.deepEqual(gateLamp(true, false), { word: 'gate open', tone: 'g' });
  assert.deepEqual(gateLamp(true, true), { word: 'gate closed', tone: 'off' });
});

test('clock is 24-hour hh:mm:ss', () => {
  assert.equal(formatClock(new Date(2026, 10, 19, 9, 5, 7)), '09:05:07');
});

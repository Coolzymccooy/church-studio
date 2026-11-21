import assert from 'node:assert/strict';
import test from 'node:test';
import {
  DB_OFF, clipUntil, fitStrip, faderDb, faderPosition, inMixDb, lcdReadout, nudge, orderStrips, peakLabel,
  initialMeter, stepMeter, withDetent,
} from '../../src-tauri/src/remote_mixer/web/mixmath.js';

test('the fader taper puts unity three quarters up and off at the bottom', () => {
  assert.equal(faderPosition(DB_OFF), 0);
  assert.equal(faderPosition(-200), 0);
  assert.equal(faderPosition(Number.NaN), 0);
  assert.equal(faderPosition(10), 1);
  assert.equal(faderPosition(0), 0.8);
  assert.ok(faderPosition(-10) > faderPosition(-20));
  for (const db of [-60, -42.5, -20, -6, -0.1, 0, 3.3, 10]) {
    assert.ok(Math.abs(faderDb(faderPosition(db)) - db) < 0.11, `${db} round-trips`);
  }
  assert.equal(faderDb(0), DB_OFF);
  assert.equal(faderDb(0.005), DB_OFF, 'the bottom millimetres are off');
  assert.equal(faderDb(1.4), 10);
});

test('the unity detent catches values near 0 dB and nothing else', () => {
  assert.equal(withDetent(0.4), 0);
  assert.equal(withDetent(-0.4), 0);
  assert.equal(withDetent(-0.6), -0.6);
  assert.equal(withDetent(DB_OFF), DB_OFF);
});

test('fine adjust steps from off to -60 and back to off at the bottom', () => {
  assert.equal(nudge(DB_OFF, 0.5), -60);
  assert.equal(nudge(DB_OFF, -1), DB_OFF);
  assert.equal(nudge(-12.5, 0.5), -12);
  assert.equal(nudge(-0.1, 0.1), 0);
  assert.equal(nudge(9.8, 1), 10);
  assert.equal(nudge(-89.6, -1), DB_OFF);
});

test('a channel reads in the mix at its tap plus its send, and silent when the send is off', () => {
  assert.equal(inMixDb(-12, -6), -18);
  assert.equal(inMixDb(-12, DB_OFF), -Infinity);
  assert.equal(inMixDb(undefined, 0), -Infinity);
});

test('meters rise fast, fall slowly and hold their peak for a second', () => {
  let meter = stepMeter(initialMeter(), -6, 0.02);
  assert.ok(meter.level > -60 && meter.level < -6);
  for (let i = 0; i < 20; i += 1) meter = stepMeter(meter, -6, 0.05);
  assert.ok(Math.abs(meter.level + 6) < 0.5);
  const peak = meter.peak;
  meter = stepMeter(meter, -60, 0.5);
  assert.equal(meter.peak, peak, 'held');
  assert.ok(meter.level < -6);
  meter = stepMeter(meter, -60, 1);
  assert.ok(meter.peak < peak, 'decays after the hold');
  const instant = stepMeter(initialMeter(), -6, 0.05, { instant: true });
  assert.equal(instant.level, -6, 'reduced motion jumps straight to the reading');
});

test('a meter that falls silent lands exactly on the floor, so the draw loop can idle', () => {
  let meter = stepMeter(initialMeter(), 0, 0.1);
  for (let i = 0; i < 60 * 600; i += 1) meter = stepMeter(meter, -Infinity, 1 / 60);
  assert.equal(meter.level, -60);
  assert.equal(meter.peak, -60);
});

test('a clip lamp latches for two seconds from the last over', () => {
  assert.equal(clipUntil(0, -3, 1000), 0);
  assert.equal(clipUntil(0, 0, 1000), 3000);
  assert.equal(clipUntil(3000, -20, 2000), 3000);
});

test('pinned channels lead in their original order', () => {
  const items = ['Pastor', 'Keys', 'Bass', 'Me'].map((name, index) => ({ name, index }));
  assert.deepEqual(orderStrips(items, new Set(['Me', 'Keys'])).map(({ name }) => name), ['Keys', 'Me', 'Pastor', 'Bass']);
  assert.deepEqual(orderStrips(items, new Set()).map(({ name }) => name), ['Pastor', 'Keys', 'Bass', 'Me']);
});

test('the readout keeps a fixed width so digits never shift', () => {
  assert.equal(lcdReadout(-12.5), '\u221212.5');
  assert.equal(lcdReadout(-4), ' −4.0');
  assert.equal(lcdReadout(0), '  0.0');
  assert.equal(lcdReadout(6), ' +6.0');
  assert.equal(lcdReadout(DB_OFF), '  Off');
  for (const db of [-60, -4, 0, 10, DB_OFF]) assert.equal(lcdReadout(db).length, 5);
  assert.equal(peakLabel(-75), '−∞');
  assert.equal(peakLabel(-6.24), '−6.2');
  assert.equal(peakLabel(1.5), '+1.5');
});

test('strips shrink to fit every channel on screen, but never below a thumb target', () => {
  // iPhone portrait: 8 channels in 400 px with 3 px gaps fit at 47 px each.
  assert.equal(fitStrip({ available: 400, count: 8, gap: 3, min: 44, max: 76 }), 47);
  assert.equal(fitStrip({ available: 1200, count: 8, gap: 3, min: 44, max: 76 }), 76, 'capped at full size');
  assert.equal(fitStrip({ available: 400, count: 16, gap: 3, min: 44, max: 76 }), 44, 'too many: scroll at the minimum');
  assert.equal(fitStrip({ available: 400, count: 0, gap: 3, min: 44, max: 76 }), 76);
});

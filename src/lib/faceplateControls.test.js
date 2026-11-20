import test from 'node:test';
import assert from 'node:assert/strict';

import {
  ARM_TIMEOUT_MS, IDLE, pressTwoPress, isArmed,
  knobFraction, knobView, knobKeyStep, knobResetValue, clampToStep, LOG_TAPER,
} from './faceplateControls.js';
import {
  METER_MIN_DB, METER_MAX_DB, meterFraction, meterZone, stepMeter, initialMeter, ZONE_AMBER_DB, ZONE_RED_DB,
} from './faceplateMeter.js';

// ---------- two-press ----------
test('first press arms and does not fire', () => {
  const r = pressTwoPress(IDLE, 1000);
  assert.equal(r.fire, false);
  assert.equal(isArmed(r.state, 1000), true);
});

test('second press inside the window fires and disarms', () => {
  const armed = pressTwoPress(IDLE, 1000).state;
  const r = pressTwoPress(armed, 1000 + ARM_TIMEOUT_MS - 1);
  assert.equal(r.fire, true);
  assert.equal(isArmed(r.state, 1000 + ARM_TIMEOUT_MS), false);
});

test('a press after the window re-arms instead of firing', () => {
  const armed = pressTwoPress(IDLE, 1000).state;
  const r = pressTwoPress(armed, 1000 + ARM_TIMEOUT_MS);
  assert.equal(r.fire, false);
  assert.equal(isArmed(r.state, 1000 + ARM_TIMEOUT_MS), true);
});

test('arm drops by itself after 3 s', () => {
  assert.equal(ARM_TIMEOUT_MS, 3000);
  const armed = pressTwoPress(IDLE, 0).state;
  assert.equal(isArmed(armed, 2999), true);
  assert.equal(isArmed(armed, 3000), false);
});

// ---------- knob ----------
const GAIN = { min: -12, max: 12, step: 0.5 };

test('knobFraction clamps and maps the range', () => {
  assert.equal(knobFraction(-12, GAIN), 0);
  assert.equal(knobFraction(0, GAIN), 0.5);
  assert.equal(knobFraction(99, GAIN), 1);
});

test('unipolar arc grows from the 7 o\'clock stop', () => {
  const v = knobView(12, { ...GAIN, bipolar: false });
  assert.equal(v.arcStart, 135);
  assert.equal(v.arcLength.toFixed(2), '80.11');
  assert.equal(v.pointerDeg, 135);
});

test('bipolar arc grows from 12 o\'clock either way', () => {
  const centre = knobView(0, { ...GAIN, bipolar: true });
  assert.equal(centre.arcLength, 0);
  assert.equal(centre.pointerDeg, 0);
  const left = knobView(-12, { ...GAIN, bipolar: true });
  assert.equal(left.arcStart, 135);
  assert.equal(left.arcLength.toFixed(2), '40.06');
  const right = knobView(12, { ...GAIN, bipolar: true });
  assert.equal(right.arcStart, 270);
});

test('keys step the knob; shift steps ten times; Home and End jump', () => {
  assert.equal(knobKeyStep(0, GAIN, 'ArrowUp'), 0.5);
  assert.equal(knobKeyStep(0, GAIN, 'ArrowLeft'), -0.5);
  assert.equal(knobKeyStep(0, GAIN, 'ArrowUp', true), 5);
  assert.equal(knobKeyStep(0, GAIN, 'PageDown'), -2.5);
  assert.equal(knobKeyStep(3, GAIN, 'Home'), -12);
  assert.equal(knobKeyStep(3, GAIN, 'End'), 12);
  assert.equal(knobKeyStep(11.8, GAIN, 'ArrowUp'), 12);
  assert.equal(knobKeyStep(3, GAIN, 'Tab'), null);
});

test('values snap to the step and the range', () => {
  assert.equal(clampToStep(0.26, GAIN), 0.5);
  assert.equal(clampToStep(-40, GAIN), -12);
});

test('reset returns to the default, else the detent, else the bottom stop', () => {
  assert.equal(knobResetValue(GAIN, 3), 3);
  assert.equal(knobResetValue({ ...GAIN, detent: 0 }), 0);
  assert.equal(knobResetValue(GAIN), -12);
  assert.equal(knobResetValue(GAIN, 99), 12);
});

test('a log taper puts each octave on an equal turn', () => {
  const HZ = { min: 20, max: 320, step: 5, taper: LOG_TAPER };
  assert.equal(knobFraction(20, HZ), 0);
  assert.equal(knobFraction(80, HZ), 0.5);
  assert.equal(knobFraction(320, HZ), 1);
  assert.equal(knobFraction(1, HZ), 0);
});

// ---------- meter ----------
test('meter maps -60..+6 dBFS onto the ladder', () => {
  assert.equal(METER_MIN_DB, -60);
  assert.equal(METER_MAX_DB, 6);
  assert.equal(meterFraction(-60), 0);
  assert.equal(meterFraction(6), 1);
  assert.equal(meterFraction(-Infinity), 0);
  assert.equal(meterFraction(Number.NaN), 0);
});

test('zones: green below -18, amber to -3, red above', () => {
  assert.equal(ZONE_AMBER_DB, -18);
  assert.equal(ZONE_RED_DB, -3);
  assert.equal(meterZone(-30), 'g');
  assert.equal(meterZone(-18), 'a');
  assert.equal(meterZone(-3.1), 'a');
  assert.equal(meterZone(-3), 'r');
});

test('meter rises fast, falls slow, and holds the peak for a second', () => {
  let m = initialMeter();
  m = stepMeter(m, 0, 0.05);
  assert.equal(m.level, 0, 'reaches the target within one 50 ms frame');
  const peak = m.peak;
  m = stepMeter(m, -60, 0.05);
  assert.ok(m.level < 0 && m.level > -30, `falls only part way in 50 ms: ${m.level}`);
  assert.equal(m.peak, peak);
  for (let i = 0; i < 12; i += 1) m = stepMeter(m, -60, 0.1);
  assert.ok(m.peak < peak, 'peak decays after the hold');
});

test('stepMeter never mutates its input', () => {
  const m = Object.freeze(initialMeter());
  assert.doesNotThrow(() => stepMeter(m, 0, 0.05));
});

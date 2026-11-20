import test from 'node:test';
import assert from 'node:assert/strict';

import {
  DRAG_THROW_PX, DETENT_PX, knobTravelSpan, knobTravelFor, knobValueAt, knobDragTravel, knobDragValue,
  knobWheelTravel, knobWheelStep, knobTakesWheel, parseKnobText,
} from './knobTravel.js';

const GAIN = { min: -12, max: 12, step: 0.5 };
const GAIN_D = { ...GAIN, detent: 0 };
const PAN = { min: -1, max: 1, step: 0.05, bipolar: true, detent: 0 };

// ---------- drag ----------
test('dragging up 200 px sweeps the full range; down turns it down', () => {
  assert.equal(DRAG_THROW_PX, 200);
  assert.equal(knobDragValue(-12, -200, GAIN), 12);
  assert.equal(knobDragValue(0, 25, GAIN), -3);
  assert.equal(knobDragValue(12, 400, GAIN), -12);
});

test('fine mode moves a fifth as far', () => {
  assert.equal(knobDragValue(0, -50, GAIN), 6);
  assert.equal(knobDragValue(0, -50, GAIN, true), 1);
});

test('the larger axis wins: up or right turns the knob up', () => {
  assert.equal(knobDragTravel(100, 30, 5, GAIN), 130);
  assert.equal(knobDragTravel(100, -30, 5, GAIN), 70);
  assert.equal(knobDragTravel(100, 5, -30, GAIN), 130);
  assert.equal(knobDragTravel(100, 10, 10, GAIN), 90, 'a tie reads as vertical');
});

test('travel clamps, so turning back after a stop responds at once', () => {
  const span = knobTravelSpan(GAIN_D);
  assert.equal(span, DRAG_THROW_PX + DETENT_PX);
  assert.equal(knobDragTravel(span - 5, 0, -50, GAIN_D), span);
  assert.equal(knobValueAt(knobDragTravel(span, 0, 20, GAIN_D), GAIN_D), 9.5);
  assert.equal(knobDragTravel(3, 0, 50, GAIN_D), 0);
});

// ---------- detent ----------
test('the knob rests on its detent for a few pixels either way', () => {
  assert.equal(knobTravelFor(0, GAIN_D), 105);
  assert.equal(knobDragValue(0, -4, GAIN_D), 0);
  assert.equal(knobDragValue(0, 4, GAIN_D), 0);
  assert.equal(knobDragValue(0, -8, GAIN_D), 0.5);
  assert.equal(knobDragValue(0, 8, GAIN_D), -0.5);
});

test('a drag passing through the detent stops there before going on', () => {
  assert.equal(knobTravelFor(-6, GAIN_D), 50);
  assert.equal(knobTravelFor(6, GAIN_D), 160);
  assert.equal(knobDragValue(-6, -60, GAIN_D), 0);
  assert.equal(knobDragValue(-6, -70, GAIN_D), 1);
});

test('a detent on a stop is ignored', () => {
  assert.equal(knobTravelSpan({ ...GAIN, detent: -12 }), DRAG_THROW_PX);
  assert.equal(knobTravelSpan(GAIN), DRAG_THROW_PX);
});

// ---------- wheel and trackpad ----------
test('trackpad deltas are scaled; a notched wheel moves a fixed amount a click', () => {
  assert.equal(knobWheelTravel({ deltaY: -10 }), 4);
  assert.equal(knobWheelTravel({ deltaY: 10 }), -4);
  assert.equal(knobWheelTravel({ deltaY: -100 }), 5);
  assert.equal(knobWheelTravel({ deltaY: -240 }), 10);
  assert.equal(knobWheelTravel({ deltaY: -3, deltaMode: 1 }), 5, 'Firefox lines: three lines a click');
});

test('natural scrolling: fingers pushed up still turn the knob up', () => {
  assert.equal(knobWheelTravel({ deltaY: 10, inverted: true }), 4);
  assert.equal(knobWheelTravel({ deltaY: -10, inverted: true }), -4);
});

test('sideways scrolls and pinches are left to the page', () => {
  assert.equal(knobWheelTravel({ deltaX: 20, deltaY: 5 }), null);
  assert.equal(knobWheelTravel({ deltaY: -10, ctrlKey: true }), null);
  assert.equal(knobWheelTravel({ deltaY: 0 }), null);
  assert.equal(knobWheelStep(100, { deltaX: 20 }, GAIN), null);
});

test('shift+wheel that arrives sideways still reads as vertical', () => {
  assert.equal(knobWheelTravel({ deltaX: -100, deltaY: 0, shiftKey: true }), 5);
});

test('small trackpad deltas add up instead of being lost', () => {
  let travel = knobTravelFor(0, GAIN);
  travel = knobWheelStep(travel, { deltaY: -1 }, GAIN);
  assert.equal(knobValueAt(travel, GAIN), 0, 'one tiny delta does not move the value yet');
  for (let i = 0; i < 9; i += 1) travel = knobWheelStep(travel, { deltaY: -1 }, GAIN);
  assert.equal(knobValueAt(travel, GAIN), 0.5, 'ten of them move it one step');
});

test('fine wheel and clamped wheel travel', () => {
  assert.equal(knobWheelStep(100, { deltaY: -100 }, GAIN, true), 101);
  assert.equal(knobWheelStep(198, { deltaY: -100 }, GAIN), 200);
  assert.equal(knobWheelStep(2, { deltaY: 100 }, GAIN), 0);
});

test('a page scroll passing over a knob keeps scrolling the page', () => {
  assert.equal(knobTakesWheel(1000, 800), false);
  assert.equal(knobTakesWheel(1000, 700), true);
  assert.equal(knobTakesWheel(1000, -Infinity), true);
});

// ---------- typed entry ----------
test('typed numbers snap and clamp; units are ignored', () => {
  assert.equal(parseKnobText('+3 dB', GAIN), 3);
  assert.equal(parseKnobText('3.26', GAIN), 3.5);
  assert.equal(parseKnobText('.5', GAIN), 0.5);
  assert.equal(parseKnobText('99', GAIN), 12);
  assert.equal(parseKnobText('-inf', GAIN), -12);
  assert.equal(parseKnobText('abc', GAIN), null);
  assert.equal(parseKnobText('  ', GAIN), null);
});

test('pan reads c, l30 and r100', () => {
  assert.equal(parseKnobText('c', PAN), 0);
  assert.equal(parseKnobText('l30', PAN), -0.3);
  assert.equal(parseKnobText('R 50', PAN), 0.5);
  assert.equal(parseKnobText('r100', PAN), 1);
  assert.equal(parseKnobText('c', GAIN), null, 'centre only means something on a bipolar knob');
});

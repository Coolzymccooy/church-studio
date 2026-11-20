import test from 'node:test';
import assert from 'node:assert/strict';

import { paramRange } from './mixerEngine.js';
import { knobFraction } from './faceplateControls.js';
import { knobDragValue } from './knobTravel.js';
import {
  STRIP_KNOBS, STRIP_ROWS, STRIP_COLOURS, stripColour, stripGateTone, grText, panText, sendText, compactDb,
  DUCK_KNOB, hasAutomix, reductionText,
  BUS_LIMITER, busLabel,
} from './faceplateStrip.js';

test('every strip knob uses the contract range for its engine parameter', () => {
  for (const [id, k] of Object.entries(STRIP_KNOBS)) {
    assert.deepEqual([k.min, k.max], paramRange(k.param), id);
    assert.ok(k.defaultValue >= k.min && k.defaultValue <= k.max, `${id} default`);
    assert.ok(k.label.length <= 6, `${id} label is short`);
  }
});

test('knob defaults match the engine defaults', () => {
  const byParam = Object.fromEntries(Object.values(STRIP_KNOBS).map((k) => [k.param, k.defaultValue]));
  assert.equal(byParam.hpf_hz, 80);
  assert.equal(byParam.gate_threshold_db, -45);
  assert.equal(byParam.comp_threshold_db, -18);
  assert.equal(byParam.comp_ratio, 3);
  assert.equal(byParam.trim_db, 0);
});

test('the strip layout lists every knob exactly once', () => {
  const used = STRIP_ROWS.flat();
  assert.deepEqual([...used].sort(), Object.keys(STRIP_KNOBS).sort());
});

test('a knob greys out when its stage is off', () => {
  const strip = { hpf_enabled: false, gate_enabled: true, comp_enabled: false };
  assert.equal(STRIP_KNOBS.hz.off(strip), true);
  assert.equal(STRIP_KNOBS.gthr.off(strip), false);
  assert.equal(STRIP_KNOBS.cthr.off(strip), true);
  assert.equal(STRIP_KNOBS.ratio.off(strip), true);
  assert.equal(STRIP_KNOBS.trim.off(strip), false);
});

test('strips cycle through the channel colours', () => {
  assert.equal(stripColour(0), STRIP_COLOURS[0]);
  assert.equal(stripColour(STRIP_COLOURS.length), STRIP_COLOURS[0]);
  assert.equal(new Set(STRIP_COLOURS).size, STRIP_COLOURS.length);
});

test('gate lamp: off when the gate is off, green open, amber closed', () => {
  assert.equal(stripGateTone(false, true), 'off');
  assert.equal(stripGateTone(true, true), 'g');
  assert.equal(stripGateTone(true, false), 'a');
  assert.equal(stripGateTone(true, undefined), 'g');
});

test('gain reduction text', () => {
  assert.equal(grText(0), 'gr 0.0');
  assert.equal(grText(0.05), 'gr 0.0');
  assert.equal(grText(3.24), 'gr -3.2');
  assert.equal(grText(undefined), 'gr 0.0');
  assert.equal(grText(9.94), 'gr -9.9');
  assert.equal(grText(9.96), 'gr -10');
  assert.equal(grText(12.5), 'gr -13');
  assert.equal(grText(250), 'gr -99');
  for (const v of [0.3, 5.55, 9.96, 12.5, 40, 250]) assert.ok(grText(v).length <= 7, grText(v));
});

test('pan and send text', () => {
  assert.equal(panText(0), 'c');
  assert.equal(panText(0.01), 'c');
  assert.equal(panText(-0.5), 'l50');
  assert.equal(panText(1), 'r100');
  assert.equal(sendText(-90), '-inf');
  assert.equal(sendText(-6), '-6.0');
  assert.equal(sendText(2), '+2.0');
  assert.equal(sendText(-60), '-60');
  assert.equal(sendText(10), '+10');
});

test('knob values never exceed four characters, so they fit under a small knob', () => {
  for (const [id, k] of Object.entries(STRIP_KNOBS)) {
    for (let v = k.min; v <= k.max; v += k.step) {
      const text = k.fmt(Number(v.toFixed(3)));
      assert.ok(text.length <= 4, `${id} ${v} -> ${text}`);
    }
  }
  assert.equal(compactDb(-9.96), '-10');
  assert.equal(compactDb(9.94), '+9.9');
  assert.equal(compactDb(0), '0.0');
});

test('bus limiter knob uses the contract range and the engine default of -1 dB', () => {
  assert.deepEqual([BUS_LIMITER.min, BUS_LIMITER.max], paramRange('limiter_ceiling_db'));
  assert.equal(BUS_LIMITER.defaultValue, -1);
  assert.equal(BUS_LIMITER.fmt(-1), '-1.0');
  assert.equal(BUS_LIMITER.fmt(-12), '-12');
});

test('bus labels are short and lower case', () => {
  assert.equal(busLabel('main'), 'main');
  assert.equal(busLabel('stream'), 'stream');
  assert.equal(busLabel('monitor'), 'mon');
  assert.equal(busLabel('aux9'), 'aux9');
});

test('reductionText fits the strip like grText', () => {
  assert.equal(reductionText('mix', 0), 'mix 0.0');
  assert.equal(reductionText('mix', 3.24), 'mix -3.2');
  assert.equal(reductionText('duck', 12), 'duck -12');
  assert.equal(reductionText('duck', 0.3), 'duck -1');
  assert.equal(reductionText('duck', 0), 'duck 0');
  assert.equal(reductionText('duck', 400), 'duck -99');
  for (const v of [0.3, 9.96, 12, 40]) assert.ok(reductionText('duck', v).length <= 8, reductionText('duck', v));
});

test('hasAutomix only when the engine reports both keys', () => {
  assert.equal(hasAutomix({ automix: false, duck: false }), true);
  assert.equal(hasAutomix({ automix: false }), false);
  assert.equal(hasAutomix({}), false);
  assert.equal(hasAutomix(null), false);
});

test('duck knob uses the contract range', () => {
  assert.deepEqual([DUCK_KNOB.min, DUCK_KNOB.max, DUCK_KNOB.defaultValue], [0, 40, 12]);
});

test('knob definitions: sends turn like faders, hz turns by octaves, 0 dB and centre are detents', () => {
  const { main, hz, trim, pan, mid } = STRIP_KNOBS;
  assert.equal(knobFraction(0, main), 0.75);
  assert.equal(knobFraction(10, main), 1);
  assert.equal(knobFraction(40, hz).toFixed(3), '0.231');
  for (const k of [main, trim, pan, mid]) assert.equal(k.detent, 0, k.label);
  assert.equal(knobDragValue(0, -3, main), 0, 'a send rests at 0 dB');
  assert.equal(knobDragValue(-90, -100, main), -30);
});

import test from 'node:test';
import assert from 'node:assert/strict';
import { createMixerController, createMockMixerController, paramRange } from './mixerEngine.js';
import * as fx from './mixerFx.js';

test('tap tempo uses recent intervals and restarts after an invalid gap', () => {
  const tap = fx.createTapTempo();
  assert.equal(tap(1000), null);
  assert.equal(tap(1500), 120);
  assert.equal(tap(2004), 120);
  assert.equal(tap(2500), 120);
  assert.equal(tap(5000), null);
  assert.equal(tap(6000), 60);
  tap.reset();
  assert.equal(tap(6500), null);
});

test('FX sends and returns round-trip independently through demo scenes', async () => {
  const desk = createMockMixerController();
  const initial = await desk.getState();
  assert.deepEqual(paramRange('send_fx_a_db'), [-90, 10]);
  assert.deepEqual(initial.fxReturns.map((r) => r.id), ['fx_a', 'fx_b']);
  assert.equal(initial.strips[0].send_fx_b_db, -90);
  await desk.setStripParam(0, 'send_fx_b_db', -10);
  await desk.setFxParam('fx_b', 'tempo_bpm', 200);
  await desk.setFxBool('fx_a', 'mute', true);
  await desk.saveScene('FX');
  await desk.setFxParam('fx_b', 'tempo_bpm', 999);
  assert.equal((await desk.getState()).fxReturns[1].tempo_bpm, 240);
  await assert.rejects(desk.setFxParam('fx_a', 'tempo_bpm', 100));
  const scene = await desk.loadScene('FX');
  assert.equal(scene.fxReturns[1].tempo_bpm, 200);
  assert.equal(scene.fxReturns[0].mute, true);
  assert.equal(scene.strips[0].send_fx_b_db, -10);
});

test('desktop FX writes invoke their typed endpoints', async () => {
  const calls = [];
  const desk = createMixerController({ invoke: (...args) => calls.push(args) });
  await desk.setFxParam('fx_a', 'decay_s', 2.2);
  await desk.setFxBool('fx_b', 'solo', true);
  assert.deepEqual(calls, [
    ['mixer_set_fx_param', { id: 'fx_a', key: 'decay_s', value: 2.2 }],
    ['mixer_set_fx_bool', { id: 'fx_b', key: 'solo', value: true }],
  ]);
});

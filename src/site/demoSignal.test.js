import test from 'node:test';
import assert from 'node:assert/strict';
import { demoLevelDb, DEMO_FLOOR_DB, DEMO_CEIL_DB } from './demoSignal.js';

test('the demo level stays between the floor and the ceiling', () => {
  for (let seed = 0; seed < 5; seed += 1) {
    for (let t = 0; t < 120; t += 0.037) {
      const db = demoLevelDb(t, seed);
      assert.ok(Number.isFinite(db));
      assert.ok(db >= DEMO_FLOOR_DB && db <= DEMO_CEIL_DB, `${db} at t=${t} seed=${seed}`);
    }
  }
});

test('the demo never reaches the red zone', () => {
  assert.ok(DEMO_CEIL_DB < -3);
});

test('each seed moves differently', () => {
  assert.notEqual(demoLevelDb(1.5, 0), demoLevelDb(1.5, 1));
});

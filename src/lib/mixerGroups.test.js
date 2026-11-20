import assert from 'node:assert/strict';
import test from 'node:test';
import { createMockMixerController, createMixerController } from './mixerEngine.js';
import { applyGroupChange, hasGroup } from './mixerGroups.js';

test('group changes preserve member fader values and overlap mute reasons', async () => {
  const desk = createMockMixerController();
  const initial = await desk.getState();
  const assigned = applyGroupChange(initial, { type: 'membership', kind: 'dca', index: 7, strip: 0, enabled: true });
  const assignedBoth = applyGroupChange(assigned, { type: 'membership', kind: 'mute', index: 1, strip: 0, enabled: true });
  const moved = applyGroupChange(assignedBoth, { type: 'fader', index: 7, db: -12 });
  const muted = applyGroupChange(moved, { type: 'dcaMute', index: 7, enabled: true });
  const overlap = applyGroupChange(muted, { type: 'muteGroup', index: 1, enabled: true });
  const release = applyGroupChange(overlap, { type: 'dcaMute', index: 7, enabled: false });
  assert.equal(hasGroup(initial.strips[0].group_membership.dca, 7), false);
  assert.equal(moved.groups.faders[7], -12);
  assert.equal(moved.strips[0].fader_db, initial.strips[0].fader_db);
  assert.equal(release.strips[0].group_muted, true);
  assert.equal(release.strips[1].group_muted, false);
});

test('mock and desktop carry the same typed group command', async () => {
  const change = { type: 'fader', index: 2, db: -7.5 };
  const calls = [];
  await createMixerController({ invoke: (...args) => calls.push(args) }).setGroup(change);
  assert.deepEqual(calls, [['mixer_set_group', { change }]]);
  const mock = createMockMixerController();
  await mock.setGroup(change);
  await mock.saveScene('Groups');
  await mock.setGroup({ ...change, db: 3 });
  assert.equal((await mock.loadScene('Groups')).groups.faders[2], -7.5);
  await assert.rejects(mock.setGroup({ ...change, index: 8 }));
});

test('a group mute keeps the same strip height', async () => {
  const { stripRowPx } = await import('./stripScale.js');
  const mixer = createMockMixerController();
  const { strips } = await mixer.getState();
  const muted = strips.map((strip, i) => ({ ...strip, group_muted: i === 0 }));
  assert.equal(stripRowPx(strips), stripRowPx(muted));
});

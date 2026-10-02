import test from 'node:test';
import assert from 'node:assert/strict';

import {
  appendActivity,
  bridgeUrl,
  createLinkController,
  formatTimeAgo,
  ITEM_TYPE_OPTIONS,
  itemTypeOptions,
  sceneSelectValue,
  moveRule,
  newRule,
  updateRule,
  validateRule,
  validateRules,
} from './linkControl.js';

function recordingInvoke() {
  const calls = [];
  const invoke = async (command, args) => {
    calls.push([command, args]);
    return null;
  };
  return { calls, invoke };
}

const defaultRule = {
  id: 'lyrics-worship',
  enabled: true,
  when: { event: 'lumina.slide.changed', contentKind: 'lyrics', itemType: null },
  then: { loadScene: 'Worship' },
};

test('createLinkController uses the exact command names and argument shapes', async () => {
  const { calls, invoke } = recordingInvoke();
  const link = createLinkController({ invoke, listen: async () => () => {} });
  await link.getConfig();
  await link.setEnabled(1);
  await link.setRules([defaultRule]);
  await link.regenerateToken();
  await link.getActivity();
  await link.undo(7);
  await link.setAutomationPaused(true);
  await link.listScenes();
  assert.deepEqual(calls, [
    ['link_get_config', undefined],
    ['link_set_enabled', { enabled: true }],
    ['link_set_rules', { rules: [defaultRule] }],
    ['link_regenerate_token', undefined],
    ['link_activity', undefined],
    ['link_undo', { entryId: 7 }],
    ['link_set_automation_paused', { paused: true }],
    ['mixer_list_scenes', undefined],
  ]);
});

test('createLinkController subscribes to link-activity and link-status', async () => {
  const handlers = {};
  let unlistened = 0;
  const listen = async (name, handler) => {
    handlers[name] = handler;
    return () => { unlistened += 1; };
  };
  const link = createLinkController({ invoke: async () => null, listen });
  const got = [];
  const a = link.subscribeActivity((p) => got.push(['a', p]));
  const s = link.subscribeStatus((p) => got.push(['s', p]));
  await a.ready;
  await s.ready;
  handlers['link-activity']({ payload: { id: 1 } });
  handlers['link-status']({ payload: { paused: true } });
  assert.deepEqual(got, [['a', { id: 1 }], ['s', { paused: true }]]);
  a.dispose();
  s.dispose();
  assert.equal(unlistened, 2);
  handlers['link-activity']({ payload: { id: 2 } });
  assert.equal(got.length, 2);
});

test('formatTimeAgo covers never, seconds, minutes, hours and days', () => {
  const now = 1_000_000_000;
  assert.equal(formatTimeAgo(null, now), 'never');
  assert.equal(formatTimeAgo(0, now), 'never');
  assert.equal(formatTimeAgo(now - 500, now), 'just now');
  assert.equal(formatTimeAgo(now + 5000, now), 'just now');
  assert.equal(formatTimeAgo(now - 12_000, now), '12s ago');
  assert.equal(formatTimeAgo(now - 3 * 60_000, now), '3m ago');
  assert.equal(formatTimeAgo(now - 2 * 3_600_000, now), '2h ago');
  assert.equal(formatTimeAgo(now - 4 * 86_400_000, now), '4d ago');
});

test('bridgeUrl builds the loopback endpoint', () => {
  assert.equal(bridgeUrl(4460), 'http://127.0.0.1:4460/api/lumina/bridge');
  assert.equal(bridgeUrl(null), null);
  assert.equal(bridgeUrl(0), null);
});

test('validateRule checks event, scene name and scene existence', () => {
  assert.deepEqual(validateRule(defaultRule), []);
  assert.deepEqual(validateRule(defaultRule, ['worship', 'Sermon']), []);
  assert.match(validateRule(defaultRule, ['Sermon'])[0], /does not exist/);
  const noScene = updateRule([defaultRule], 0, { then: { loadScene: '' } })[0];
  assert.deepEqual(validateRule(noScene), ['Pick a scene to load.']);
  const badScene = updateRule([defaultRule], 0, { then: { loadScene: '../x' } })[0];
  assert.deepEqual(validateRule(badScene), ['Pick a scene to load.']);
  const noEvent = updateRule([defaultRule], 0, { when: { event: '' } })[0];
  assert.deepEqual(validateRule(noEvent), ['Pick a Lumina event.']);
  assert.ok(validateRule({}).length >= 3);
});

test('validateRules reports duplicates and per-rule errors with positions', () => {
  assert.deepEqual(validateRules([defaultRule]), []);
  const errors = validateRules([defaultRule, { ...defaultRule }]);
  assert.deepEqual(errors, ['Rule 2: duplicate id.']);
  assert.deepEqual(validateRules('nope'), ['Rules must be a list.']);
  const many = Array.from({ length: 51 }, (_, i) => ({ ...defaultRule, id: `r${i}` }));
  assert.match(validateRules(many)[0], /At most 50/);
});

test('newRule picks an unused id', () => {
  const rules = [{ ...defaultRule, id: 'rule-2' }];
  const rule = newRule(rules, 'Sermon');
  assert.equal(rule.id, 'rule-3');
  assert.equal(rule.then.loadScene, 'Sermon');
  assert.equal(rule.when.event, 'lumina.slide.changed');
  assert.equal(newRule([]).id, 'rule-1');
});

test('moveRule reorders without mutating and clamps at the ends', () => {
  const rules = ['a', 'b', 'c'];
  assert.deepEqual(moveRule(rules, 0, 1), ['b', 'a', 'c']);
  assert.deepEqual(moveRule(rules, 2, -1), ['a', 'c', 'b']);
  assert.deepEqual(moveRule(rules, 0, -1), ['a', 'b', 'c']);
  assert.deepEqual(moveRule(rules, 2, 1), ['a', 'b', 'c']);
  assert.deepEqual(rules, ['a', 'b', 'c']);
});

test('updateRule merges nested when/then without mutating', () => {
  const rules = [defaultRule];
  const next = updateRule(rules, 0, { when: { contentKind: 'scripture' } });
  assert.equal(next[0].when.contentKind, 'scripture');
  assert.equal(next[0].when.event, 'lumina.slide.changed');
  assert.equal(rules[0].when.contentKind, 'lyrics');
});

test('appendActivity prepends, de-duplicates and caps', () => {
  const list = appendActivity([{ id: 1 }, { id: 2 }], { id: 3 }, 2);
  assert.deepEqual(list, [{ id: 3 }, { id: 1 }]);
  assert.deepEqual(appendActivity(list, { id: 3 }), [{ id: 3 }, { id: 1 }]);
  assert.deepEqual(appendActivity(null, { id: 9 }), [{ id: 9 }]);
});

test('itemTypeOptions offers Lumina item types and keeps an unknown saved value', () => {
  assert.deepEqual(itemTypeOptions(null), ITEM_TYPE_OPTIONS);
  assert.ok(ITEM_TYPE_OPTIONS.includes('SONG') && ITEM_TYPE_OPTIONS.includes(''));
  assert.deepEqual(itemTypeOptions('song'), ITEM_TYPE_OPTIONS);
  assert.deepEqual(itemTypeOptions('PODCAST'), [...ITEM_TYPE_OPTIONS, 'PODCAST']);
  const [rule] = updateRule([{ id: 'r1', when: { event: 'lumina.item.started', contentKind: null, itemType: null }, then: { loadScene: 'Worship' } }], 0, { when: { itemType: 'SONG' } });
  assert.equal(rule.when.itemType, 'SONG');
  assert.equal(rule.when.event, 'lumina.item.started');
});

test('sceneSelectValue shows the saved spelling of a case-insensitive match', () => {
  assert.equal(sceneSelectValue(['worship', 'Sermon'], 'Worship'), 'worship');
  assert.equal(sceneSelectValue(['worship'], 'Walk-in'), 'Walk-in');
  assert.equal(sceneSelectValue(['worship'], ''), '');
  assert.equal(sceneSelectValue(null, 'Sermon'), 'Sermon');
});

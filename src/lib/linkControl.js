/**
 * linkControl.js — Tiwaton Link (Lumina → Studio automation) client helpers.
 *
 * createLinkController({ invoke, listen }) wraps the `link_*` Tauri commands
 * (see src-tauri/src/link/commands.rs) plus the `link-activity` and
 * `link-status` events. The rest are pure helpers for the Link panel.
 */
import { subscribeEvent } from './mixerEngine.js';

export const LINK_EVENTS = {
  activity: 'link-activity',
  status: 'link-status',
};

/** Lumina events a rule can react to (value, label). */
export const RULE_EVENT_OPTIONS = [
  { value: 'lumina.slide.changed', label: 'Slide changed' },
  { value: 'lumina.item.started', label: 'Item started' },
  { value: 'lumina.countdown.started', label: 'Countdown started' },
  { value: 'lumina.countdown.ended', label: 'Countdown ended' },
  { value: 'lumina.service.mode.changed', label: 'Service mode changed' },
  { value: 'lumina.scene.switch', label: 'Scene switch' },
];

/** Lumina `liveContent.contentKind` values ('' = any). */
export const CONTENT_KIND_OPTIONS = [
  '', 'lyrics', 'scripture', 'designed', 'media', 'announcement', 'blackout', 'idle',
];

/** Lumina's `ItemType` values (matched case-insensitively); '' means any. */
export const ITEM_TYPE_OPTIONS = [
  '', 'SONG', 'HYMN', 'SCRIPTURE', 'BIBLE', 'MEDIA', 'ANNOUNCEMENT',
];

/** Item-type choices for a rule, keeping a saved value Lumina may add later. */
export function itemTypeOptions(current) {
  const known = ITEM_TYPE_OPTIONS.some((v) => v.toLowerCase() === String(current ?? '').toLowerCase());
  return known || !current ? ITEM_TYPE_OPTIONS : [...ITEM_TYPE_OPTIONS, current];
}

const SCENE_NAME_RE = /^[A-Za-z0-9 _-]{1,40}$/;
const EVENT_NAME_RE = /^[A-Za-z0-9._-]{1,128}$/;
export const MAX_RULES = 50;

export function createLinkController({ invoke, listen }) {
  return {
    getConfig() {
      return invoke('link_get_config');
    },
    setEnabled(enabled) {
      return invoke('link_set_enabled', { enabled: Boolean(enabled) });
    },
    setRules(rules) {
      return invoke('link_set_rules', { rules });
    },
    regenerateToken() {
      return invoke('link_regenerate_token');
    },
    getActivity() {
      return invoke('link_activity');
    },
    undo(entryId) {
      return invoke('link_undo', { entryId });
    },
    setAutomationPaused(paused) {
      return invoke('link_set_automation_paused', { paused: Boolean(paused) });
    },
    listScenes() {
      return invoke('mixer_list_scenes');
    },
    subscribeActivity(cb) {
      return subscribeEvent(listen, LINK_EVENTS.activity, cb);
    },
    subscribeStatus(cb) {
      return subscribeEvent(listen, LINK_EVENTS.status, cb);
    },
  };
}

/** "never" | "just now" | "12s ago" | "3m ago" | "2h ago" | "4d ago". */
export function formatTimeAgo(ts, now = Date.now()) {
  if (!Number.isFinite(ts) || ts <= 0) return 'never';
  const seconds = Math.max(0, Math.floor((now - ts) / 1000));
  if (seconds < 2) return 'just now';
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

export function bridgeUrl(port) {
  return Number.isInteger(port) && port > 0
    ? `http://127.0.0.1:${port}/api/lumina/bridge`
    : null;
}

/** Errors for one rule (empty array = valid). Mirrors rules.rs validation. */
export function validateRule(rule, availableScenes = null) {
  const errors = [];
  const event = String(rule?.when?.event ?? '').trim();
  const scene = String(rule?.then?.loadScene ?? '').trim();
  if (!String(rule?.id ?? '').trim()) errors.push('Rule needs an id.');
  if (!EVENT_NAME_RE.test(event)) errors.push('Pick a Lumina event.');
  if (!SCENE_NAME_RE.test(scene)) {
    errors.push('Pick a scene to load.');
  } else if (Array.isArray(availableScenes)
    && !availableScenes.some((s) => s.toLowerCase() === scene.toLowerCase())) {
    errors.push(`Scene "${scene}" does not exist yet; save it in the mixer.`);
  }
  return errors;
}

/** Errors for the whole list (duplicate ids, limits, per-rule errors). */
export function validateRules(rules, availableScenes = null) {
  if (!Array.isArray(rules)) return ['Rules must be a list.'];
  const errors = [];
  if (rules.length > MAX_RULES) errors.push(`At most ${MAX_RULES} rules.`);
  const seen = new Set();
  rules.forEach((rule, i) => {
    const id = String(rule?.id ?? '').trim();
    if (id && seen.has(id)) errors.push(`Rule ${i + 1}: duplicate id.`);
    seen.add(id);
    validateRule(rule, availableScenes).forEach((e) => errors.push(`Rule ${i + 1}: ${e}`));
  });
  return errors;
}

/** A new rule with an id not used by `rules`. */
export function newRule(rules, scene = '') {
  const ids = new Set((rules ?? []).map((r) => r.id));
  let n = (rules?.length ?? 0) + 1;
  while (ids.has(`rule-${n}`)) n += 1;
  return {
    id: `rule-${n}`,
    enabled: true,
    when: { event: 'lumina.slide.changed', contentKind: null, itemType: null },
    then: { loadScene: scene },
  };
}

/** Copy of `rules` with the rule at `index` moved by `delta` (clamped). */
export function moveRule(rules, index, delta) {
  const target = index + delta;
  if (index < 0 || index >= rules.length || target < 0 || target >= rules.length) {
    return rules.slice();
  }
  const next = rules.slice();
  const [moved] = next.splice(index, 1);
  next.splice(target, 0, moved);
  return next;
}

/** Copy of `rules` with `patch` merged into the rule at `index`. */
export function updateRule(rules, index, patch) {
  return rules.map((rule, i) => {
    if (i !== index) return rule;
    return {
      ...rule,
      ...patch,
      when: { ...rule.when, ...(patch.when ?? {}) },
      then: { ...rule.then, ...(patch.then ?? {}) },
    };
  });
}

/** Prepend one activity entry to a list, keeping at most `limit`. */
export function appendActivity(entries, entry, limit = 50) {
  const rest = (entries ?? []).filter((e) => e.id !== entry.id);
  return [entry, ...rest].slice(0, limit);
}

export function describeLinkError(err, fallback = 'Tiwaton Link command failed.') {
  if (!err) return fallback;
  if (typeof err === 'string') return err;
  if (typeof err.message === 'string' && err.message) return err.message;
  return fallback;
}

/**
 * The scene-select value for a rule: the saved scene's own spelling when it
 * matches case-insensitively (as the rule engine does), else the rule's text.
 */
export function sceneSelectValue(scenes, current) {
  if (!current) return '';
  const wanted = String(current).trim().toLowerCase();
  const match = (scenes ?? []).find((s) => String(s).trim().toLowerCase() === wanted);
  return match ?? current;
}

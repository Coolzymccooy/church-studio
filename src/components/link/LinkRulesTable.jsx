import { useState } from 'react';
import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-react';

import {
  CONTENT_KIND_OPTIONS,
  RULE_EVENT_OPTIONS,
  itemTypeOptions,
  moveRule,
  newRule,
  updateRule,
  validateRule,
  validateRules,
} from '../../lib/linkControl.js';

const selectClass = 'w-full bg-slate-900 border border-slate-700 rounded px-1.5 py-1 text-[10px] text-slate-200';

function eventOptions(current) {
  const known = RULE_EVENT_OPTIONS.some((o) => o.value === current);
  return known || !current
    ? RULE_EVENT_OPTIONS
    : [...RULE_EVENT_OPTIONS, { value: current, label: current }];
}

function sceneOptions(scenes, current) {
  const exists = scenes.some((s) => s.toLowerCase() === String(current).toLowerCase());
  return exists || !current ? scenes : [...scenes, current];
}

function RuleRow({ rule, index, count, scenes, onChange, onMove, onRemove }) {
  const warnings = validateRule(rule, scenes);
  return (
    <tr className="border-t border-slate-800 align-top">
      <td className="py-1.5 pr-1">
        <input
          type="checkbox"
          checked={rule.enabled !== false}
          onChange={(e) => onChange({ enabled: e.target.checked })}
          title="Rule enabled"
        />
      </td>
      <td className="py-1.5 pr-1">
        <select
          className={selectClass}
          value={rule.when.event}
          onChange={(e) => onChange({ when: { event: e.target.value } })}
        >
          {eventOptions(rule.when.event).map((o) => (
            <option key={o.value} value={o.value}>{o.label}</option>
          ))}
        </select>
      </td>
      <td className="py-1.5 pr-1">
        <select
          className={selectClass}
          value={rule.when.contentKind ?? ''}
          onChange={(e) => onChange({ when: { contentKind: e.target.value || null } })}
        >
          {CONTENT_KIND_OPTIONS.map((kind) => (
            <option key={kind || 'any'} value={kind}>{kind || 'any'}</option>
          ))}
        </select>
      </td>
      <td className="py-1.5 pr-1">
        <select
          className={selectClass}
          value={rule.when.itemType ?? ''}
          onChange={(e) => onChange({ when: { itemType: e.target.value || null } })}
          title="Lumina item type (e.g. song or sermon scripture)"
        >
          {itemTypeOptions(rule.when.itemType).map((type) => (
            <option key={type || 'any'} value={type}>{type ? type.toLowerCase() : 'any'}</option>
          ))}
        </select>
      </td>
      <td className="py-1.5 pr-1">
        <select
          className={selectClass}
          value={rule.then.loadScene}
          onChange={(e) => onChange({ then: { loadScene: e.target.value } })}
        >
          <option value="">Choose scene…</option>
          {sceneOptions(scenes, rule.then.loadScene).map((s) => (
            <option key={s} value={s}>{s}</option>
          ))}
        </select>
        {warnings.length > 0 && (
          <p className="mt-0.5 text-[9px] text-amber-400">{warnings[0]}</p>
        )}
      </td>
      <td className="py-1.5 whitespace-nowrap">
        <button type="button" disabled={index === 0} onClick={() => onMove(-1)} className="p-1 text-slate-400 disabled:opacity-30" title="Move up"><ArrowUp size={11} /></button>
        <button type="button" disabled={index === count - 1} onClick={() => onMove(1)} className="p-1 text-slate-400 disabled:opacity-30" title="Move down"><ArrowDown size={11} /></button>
        <button type="button" onClick={onRemove} className="p-1 text-slate-400 hover:text-red-400" title="Remove rule"><Trash2 size={11} /></button>
      </td>
    </tr>
  );
}

/**
 * LinkRulesTable — edits a draft copy of the rules; nothing is sent until
 * "Save rules". First matching rule wins, so order matters.
 */
export default function LinkRulesTable({ rules, scenes, onSave }) {
  const [draft, setDraft] = useState(rules);
  const [saving, setSaving] = useState(false);
  const blocking = validateRules(draft).filter((e) => !e.includes('does not exist'));
  const dirty = JSON.stringify(draft) !== JSON.stringify(rules);

  const save = async () => {
    setSaving(true);
    await onSave(draft);
    setSaving(false);
  };

  return (
    <div>
      <table className="w-full text-left text-[10px] text-slate-300">
        <thead className="text-[9px] uppercase tracking-wider text-slate-500">
          <tr>
            <th className="w-6" />
            <th className="pb-1">When Lumina…</th>
            <th className="pb-1">Content</th>
            <th className="pb-1">Item</th>
            <th className="pb-1">Then load scene</th>
            <th className="w-20" />
          </tr>
        </thead>
        <tbody>
          {draft.map((rule, i) => (
            <RuleRow
              key={rule.id}
              rule={rule}
              index={i}
              count={draft.length}
              scenes={scenes}
              onChange={(patch) => setDraft((prev) => updateRule(prev, i, patch))}
              onMove={(delta) => setDraft((prev) => moveRule(prev, i, delta))}
              onRemove={() => setDraft((prev) => prev.filter((_, j) => j !== i))}
            />
          ))}
        </tbody>
      </table>
      {blocking.length > 0 && <p className="mt-1 text-[10px] text-red-400">{blocking[0]}</p>}
      <div className="mt-2 flex items-center gap-2">
        <button
          type="button"
          onClick={() => setDraft((prev) => [...prev, newRule(prev, scenes[0] ?? '')])}
          className="flex items-center gap-1 px-2 py-1 rounded border border-slate-700 text-[10px] text-slate-300 hover:text-white"
        >
          <Plus size={11} /> Add rule
        </button>
        <button
          type="button"
          disabled={!dirty || blocking.length > 0 || saving}
          onClick={save}
          className="px-2 py-1 rounded text-[10px] font-semibold text-black disabled:opacity-40"
          style={{ background: 'var(--accent)' }}
        >
          {saving ? 'Saving…' : 'Save rules'}
        </button>
        {dirty && (
          <button type="button" onClick={() => setDraft(rules)} className="text-[10px] text-slate-500 hover:text-white">
            Discard changes
          </button>
        )}
      </div>
    </div>
  );
}

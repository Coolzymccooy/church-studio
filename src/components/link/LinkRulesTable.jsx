import { useState } from 'react';

import {
  CONTENT_KIND_OPTIONS,
  RULE_EVENT_OPTIONS,
  itemTypeOptions,
  sceneSelectValue,
  moveRule,
  newRule,
  updateRule,
  validateRule,
  validateRules,
} from '../../lib/linkControl.js';
import InfoTip from '../faceplate/InfoTip.jsx';
import Key from '../faceplate/Key.jsx';

const TABLE_STYLE = { width: '100%', borderCollapse: 'collapse', textAlign: 'left', fontSize: 'var(--fp-text-sm)' };
const HEAD_STYLE = { padding: '0 4px 4px 0', fontWeight: 'inherit', textAlign: 'left' };
const CELL_STYLE = { padding: '4px 4px 4px 0', verticalAlign: 'top', borderTop: '1px solid var(--fp-seam)' };

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
  const n = index + 1;
  return (
    <tr>
      <td style={{ ...CELL_STYLE, verticalAlign: 'middle' }}>
        <input
          type="checkbox"
          className="fp-check"
          checked={rule.enabled !== false}
          onChange={(e) => onChange({ enabled: e.target.checked })}
          title="Rule enabled"
          aria-label={`rule ${n} enabled`}
        />
      </td>
      <td style={CELL_STYLE}>
        <select
          className="fp-field"
          aria-label={`rule ${n} event`}
          value={rule.when.event}
          onChange={(e) => onChange({ when: { event: e.target.value } })}
        >
          {eventOptions(rule.when.event).map((o) => (
            <option key={o.value} value={o.value}>{o.label}</option>
          ))}
        </select>
      </td>
      <td style={CELL_STYLE}>
        <select
          className="fp-field"
          aria-label={`rule ${n} content`}
          value={rule.when.contentKind ?? ''}
          onChange={(e) => onChange({ when: { contentKind: e.target.value || null } })}
        >
          {CONTENT_KIND_OPTIONS.map((kind) => (
            <option key={kind || 'any'} value={kind}>{kind || 'any'}</option>
          ))}
        </select>
      </td>
      <td style={CELL_STYLE}>
        <select
          className="fp-field"
          aria-label={`rule ${n} item type`}
          value={rule.when.itemType ?? ''}
          onChange={(e) => onChange({ when: { itemType: e.target.value || null } })}
          title="Lumina item type (e.g. song or sermon scripture)"
        >
          {itemTypeOptions(rule.when.itemType).map((type) => (
            <option key={type || 'any'} value={type}>{type ? type.toLowerCase() : 'any'}</option>
          ))}
        </select>
      </td>
      <td style={CELL_STYLE}>
        <select
          className="fp-field"
          aria-label={`rule ${n} scene`}
          value={sceneSelectValue(scenes, rule.then.loadScene)}
          onChange={(e) => onChange({ then: { loadScene: e.target.value } })}
        >
          <option value="">choose scene</option>
          {sceneOptions(scenes, rule.then.loadScene).map((s) => (
            <option key={s} value={s}>{s}</option>
          ))}
        </select>
        {warnings.length > 0 && (
          <p className="fp-sub fp-caution" style={{ margin: '2px 0 0' }}>{warnings[0]}</p>
        )}
      </td>
      <td style={{ ...CELL_STYLE, paddingRight: 0, whiteSpace: 'nowrap' }}>
        <span className="fp-keys" style={{ flexWrap: 'nowrap' }}>
          <Key disabled={index === 0} onClick={() => onMove(-1)} aria-label={`move rule ${n} up`} title="Move up">up</Key>
          <Key disabled={index === count - 1} onClick={() => onMove(1)} aria-label={`move rule ${n} down`} title="Move down">down</Key>
          <Key onClick={onRemove} aria-label={`remove rule ${n}`} title="Remove rule">remove</Key>
        </span>
      </td>
    </tr>
  );
}

/**
 * LinkRulesTable — edits a draft copy of the rules; nothing is sent until
 * "save rules". First matching rule wins, so order matters.
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
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      <div style={{ overflowX: 'auto' }}>
        <table style={TABLE_STYLE}>
          <thead className="fp-sub">
            <tr>
              <th style={{ ...HEAD_STYLE, width: 24 }}><span className="fp-sr">Enabled</span></th>
              <th style={HEAD_STYLE}>When Lumina</th>
              <th style={HEAD_STYLE}>Content</th>
              <th style={HEAD_STYLE}>
                <span style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}>
                  Item
                  <InfoTip label="Item">Lumina item type, such as song or sermon scripture.</InfoTip>
                </span>
              </th>
              <th style={HEAD_STYLE}>Load scene</th>
              <th style={HEAD_STYLE}><span className="fp-sr">Order</span></th>
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
      </div>
      {blocking.length > 0 && <p className="fp-sub fp-fault" style={{ margin: 0 }}>{blocking[0]}</p>}
      <div className="fp-keys">
        <Key onClick={() => setDraft((prev) => [...prev, newRule(prev, scenes[0] ?? '')])}>add rule</Key>
        <Key disabled={!dirty || blocking.length > 0 || saving} onClick={save}>
          {saving ? 'saving' : 'save rules'}
        </Key>
        {dirty && <Key onClick={() => setDraft(rules)}>discard changes</Key>}
      </div>
    </div>
  );
}

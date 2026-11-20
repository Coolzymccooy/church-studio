import { useState } from 'react';

import { faderDbToPosition, formatDb, positionToFaderDb } from '../../lib/mixerEngine.js';
import { GROUP_COUNT, groupLabel, hasGroup } from '../../lib/mixerGroups.js';
import { Fader, InfoTip, Key, MeterLadder } from '../faceplate/index.js';

const HEIGHT = 132;
const marks = [10, 0, -10, -30].map((db) => ({ at: faderDbToPosition(db), zero: db === 0 }));
const numbers = Array.from({ length: GROUP_COUNT }, (_, i) => i);

export default function DcaBank({ groups, labels, strips, metersRef, onChange }) {
  const [selection, setSelection] = useState('');
  const [kind, raw] = selection.split(':');
  const index = Number(raw);
  const hasSelection = Boolean(selection);

  return (
    <section className="flex-1 min-w-0 overflow-auto p-2" aria-label="Groups bank">
      <div className="flex flex-wrap gap-2" role="group" aria-label="DCA faders">
        {groups.faders.map((db, index) => {
          const label = groupLabel(labels, 'dca', index);
          const muted = hasGroup(groups.dca_muted, index);
          return (
            <div key={index} className="fp-strip" role="group" aria-label={label}>
              <div className={`fp-scrib fp-c-bus${muted ? ' fp-dim' : ''}`}>
                <span className="fp-busname truncate">{label}</span>
              </div>
              <Key aria-label={`${label} mute`} lit={muted ? 'amber' : null} pressed={muted}
                onClick={() => onChange({ type: 'dcaMute', index, enabled: !muted })}>mute</Key>
              <div className="fp-fz">
                <Fader label={`${label} fader`} colour="bus" position={faderDbToPosition(db)}
                  valueText={`${formatDb(db)} dB`} height={HEIGHT} marks={marks}
                  onChange={(position) => onChange({ type: 'fader', index, db: positionToFaderDb(position) })} />
                <MeterLadder getDb={() => metersRef.current.dca_peaks?.[index] ?? -96}
                  height={HEIGHT - 18} width={5} label={`${label} member peak`} />
              </div>
              <span className="fp-fdb">{formatDb(db)}</span>
            </div>
          );
        })}
      </div>

      <div className="fp-keys items-center mt-3" role="group" aria-label="Mute groups">
        <span className="text-fp-md">Mute groups</span>
        <InfoTip label="Mute groups">A lit group mutes its members. Choose a group in Assign to change its name or members.</InfoTip>
        {numbers.map((index) => {
          const label = groupLabel(labels, 'mute', index);
          const enabled = hasGroup(groups.mute_active, index);
          return <Key key={index} pressed={enabled} lit={enabled ? 'amber' : null}
            aria-label={label} onClick={() => onChange({ type: 'muteGroup', index, enabled: !enabled })}>
            {labels?.mute?.[index] || index + 1}
          </Key>;
        })}
        <span>
          <select className="fp-field" value={selection} aria-label="Assign group" onChange={(event) => setSelection(event.target.value)}>
            <option value="">Assign</option>
            {['dca', 'mute'].map((type) => (
              <optgroup key={type} label={type === 'dca' ? 'DCAs' : 'Mute groups'}>
                {numbers.map((n) => <option key={n} value={`${type}:${n}`}>{groupLabel(labels, type, n)}</option>)}
              </optgroup>
            ))}
          </select>
        </span>
      </div>

      {hasSelection && (
        <div className="mt-2 border-t border-fp-seam pt-2">
          <div className="fp-keys items-center mb-2">
            <label className="fp-keys items-center">
              <span>{kind === 'dca' ? 'DCA' : 'Mute group'} {index + 1}</span>
              <span>
                <input key={`${selection}:${labels?.[kind]?.[index] ?? ''}`} className="fp-field" aria-label="Group name" maxLength={24}
                  placeholder="Name" defaultValue={labels?.[kind]?.[index] ?? ''}
                  onBlur={(event) => {
                    if (event.target.value.trim() !== (labels?.[kind]?.[index] ?? '')) {
                      onChange({ type: 'rename', kind, index, name: event.target.value });
                    }
                  }}
                  onKeyDown={(event) => { if (event.key === 'Enter') event.currentTarget.blur(); }} />
              </span>
            </label>
            <Key onClick={() => setSelection('')} aria-label="Close group assignment">Close</Key>
          </div>
          <div className="fp-keys" role="group" aria-label="Strip membership">
            {strips.map((strip) => {
              const member = hasGroup(strip.group_membership[kind], index);
              return (
                <Key key={strip.index} lit={member ? 'sel' : null} pressed={member}
                  onClick={() => onChange({ type: 'membership', kind, index, strip: strip.index, enabled: !member })}>
                  {strip.name}
                </Key>
              );
            })}
          </div>
        </div>
      )}
    </section>
  );
}

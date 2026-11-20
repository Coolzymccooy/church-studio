import { ListChecks } from 'lucide-react';
import { useState } from 'react';

import { faderDbToPosition, formatDb, positionToFaderDb } from '../../lib/mixerEngine.js';
import { GROUP_COUNT, hasGroup } from '../../lib/mixerGroups.js';
import { Fader, InfoTip, Key } from '../faceplate/index.js';

const HEIGHT = 132;
const marks = [10, 0, -10, -30].map((db) => ({ at: faderDbToPosition(db), zero: db === 0 }));
const numbers = Array.from({ length: GROUP_COUNT }, (_, i) => i);

export default function DcaBank({ groups, strips, onChange }) {
  const [assignment, setAssignment] = useState(null);
  const assign = (kind, index) => {
    setAssignment((prev) => prev?.kind === kind && prev.index === index ? null : { kind, index });
  };
  const selected = (kind, index) => assignment?.kind === kind && assignment.index === index;

  return (
    <section className="flex-1 min-w-0 overflow-auto p-2" aria-label="Groups bank">
      <div className="flex flex-wrap gap-2" role="group" aria-label="DCA faders">
        {groups.faders.map((db, index) => {
          const label = `DCA ${index + 1}`;
          const muted = hasGroup(groups.dca_muted, index);
          const editing = selected('dca', index);
          return (
            <div key={index} className="fp-strip" role="group" aria-label={label}>
              <div className={`fp-scrib fp-c-bus${muted ? ' fp-dim' : ''}`}><span className="fp-busname">{label}</span></div>
              <div className="fp-krow">
                <Key aria-label={`${label} mute`} lit={muted ? 'amber' : null} pressed={muted}
                  onClick={() => onChange({ type: 'dcaMute', index, enabled: !muted })}>Mute</Key>
                <Key aria-label={`Assign ${label}`} lit={editing ? 'sel' : null} pressed={editing}
                  className="grid place-items-center" onClick={() => assign('dca', index)}>
                  <ListChecks size={14} strokeWidth={1.75} aria-hidden="true" />
                </Key>
              </div>
              <div className="fp-fz">
                <Fader label={`${label} fader`} colour="bus" position={faderDbToPosition(db)}
                  valueText={`${formatDb(db)} dB`} height={HEIGHT} marks={marks}
                  onChange={(position) => onChange({ type: 'fader', index, db: positionToFaderDb(position) })} />
              </div>
              <span className="fp-fdb">{formatDb(db)}</span>
            </div>
          );
        })}
      </div>

      <div className="fp-keys items-center mt-3" role="group" aria-label="Mute groups">
        <span className="text-fp-md">Mute groups</span>
        <InfoTip label="Mute groups">A lit group mutes its members. Choose a group in the Assign menu to change membership.</InfoTip>
        {numbers.map((index) => (
          <Key key={index} pressed={hasGroup(groups.mute_active, index)}
            lit={hasGroup(groups.mute_active, index) ? 'amber' : null}
            aria-label={`Mute group ${index + 1}`}
            onClick={() => onChange({ type: 'muteGroup', index, enabled: !hasGroup(groups.mute_active, index) })}>
            {index + 1}
          </Key>
        ))}
        <span>
          <select className="fp-field" value={assignment?.kind === 'mute' ? assignment.index : ''} aria-label="Assign mute group"
            onChange={(event) => event.target.value ? assign('mute', Number(event.target.value)) : setAssignment(null)}>
            <option value="">Assign</option>
            {numbers.map((index) => <option key={index} value={index}>{index + 1}</option>)}
          </select>
        </span>
      </div>

      {assignment && (
        <div className="mt-2 border-t border-fp-seam pt-2">
          <div className="fp-keys items-center mb-2">
            <span className="text-fp-md">{assignment.kind === 'dca' ? 'DCA' : 'Mute group'} {assignment.index + 1}</span>
            <Key onClick={() => setAssignment(null)} aria-label="Close group assignment">Close</Key>
          </div>
          <div className="fp-keys" role="group" aria-label="Strip membership">
            {strips.map((strip) => {
              const member = hasGroup(strip.group_membership[assignment.kind], assignment.index);
              return (
                <Key key={strip.index} lit={member ? 'sel' : null} pressed={member}
                  onClick={() => onChange({ type: 'membership', ...assignment, strip: strip.index, enabled: !member })}>
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

import React, { useId } from 'react';

import { buildLinkRows, setLinkEntry } from '../../lib/obsControl.js';
import InfoTip from '../faceplate/InfoTip.jsx';

const TABLE_STYLE = { width: '100%', borderCollapse: 'collapse', tableLayout: 'fixed', fontSize: 'var(--fp-text-sm)' };
const CELL_STYLE = { padding: '2px 0', textAlign: 'left', verticalAlign: 'middle' };

/** The option in `targets` matching `name` case-insensitively, or ''. */
function matchScene(targets, name) {
  const wanted = name.trim().toLowerCase();
  if (!wanted) return '';
  return targets.find((t) => t.trim().toLowerCase() === wanted) || '';
}

function LinkTable({ title, fromLabel, toLabel, sources, targets, map, onChange, disabled }) {
  const rows = buildLinkRows(sources, map);
  return (
    <div>
      <h4 className="fp-label" style={{ margin: '0 0 4px' }}>{title}</h4>
      {rows.length === 0 ? (
        <p className="fp-sub" style={{ margin: 0 }}>no {fromLabel} scenes</p>
      ) : (
        <table style={TABLE_STYLE}>
          <thead>
            <tr className="fp-sub">
              <th style={{ ...CELL_STYLE, width: '50%', fontWeight: 'inherit' }}>{fromLabel}</th>
              <th style={{ ...CELL_STYLE, fontWeight: 'inherit' }}>{toLabel}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map(({ scene, target }) => (
              <tr key={scene}>
                <td
                  style={{ ...CELL_STYLE, paddingRight: 8, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}
                  title={scene}
                >
                  {scene}
                </td>
                <td style={CELL_STYLE}>
                  <select
                    aria-label={`${toLabel} scene for ${scene}`}
                    className="fp-field"
                    value={matchScene(targets, target)}
                    disabled={disabled}
                    onChange={(e) => onChange(setLinkEntry(map, scene, e.target.value))}
                  >
                    <option value="">no link</option>
                    {targets.map((t) => (
                      <option key={t} value={t}>{t}</option>
                    ))}
                  </select>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

/** Enable toggle plus Studio → OBS and OBS → Studio mapping tables. */
export default function ObsSceneLinkSection({ link, studioScenes, obsScenes, busy, onChange }) {
  const id = useId();
  return (
    <section className="fp-mod" aria-label="Scene link">
      <div className="fp-mod-hd">
        <span>
          <input
            id={`${id}-enabled`}
            type="checkbox"
            className="fp-check"
            checked={link.enabled}
            disabled={busy}
            onChange={(e) => onChange({ ...link, enabled: e.target.checked })}
          />
          <label htmlFor={`${id}-enabled`}>Scene link</label>
          <InfoTip label="Scene link">
            Keeps Studio scenes and OBS scenes in step. Connect to OBS to see its scenes.
          </InfoTip>
        </span>
      </div>
      <div
        className="fp-mod-bd"
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(220px, 1fr))',
          gap: 16,
          opacity: link.enabled ? 1 : 0.6,
        }}
      >
        <LinkTable
          title="Studio → OBS"
          fromLabel="Studio"
          toLabel="OBS"
          sources={studioScenes}
          targets={obsScenes}
          map={link.studioToObs}
          disabled={busy}
          onChange={(studioToObs) => onChange({ ...link, studioToObs })}
        />
        <LinkTable
          title="OBS → Studio"
          fromLabel="OBS"
          toLabel="Studio"
          sources={obsScenes}
          targets={studioScenes}
          map={link.obsToStudio}
          disabled={busy}
          onChange={(obsToStudio) => onChange({ ...link, obsToStudio })}
        />
      </div>
    </section>
  );
}

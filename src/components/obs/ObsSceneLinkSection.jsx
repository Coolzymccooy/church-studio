import React from 'react';

import { buildLinkRows, setLinkEntry } from '../../lib/obsControl.js';

const selectCls = 'w-full bg-slate-950 border border-slate-700 rounded px-2 py-1 text-[11px] text-slate-200';

/** The option in `targets` matching `name` case-insensitively, or ''. */
function matchScene(targets, name) {
  const wanted = name.trim().toLowerCase();
  if (!wanted) return '';
  return targets.find((t) => t.trim().toLowerCase() === wanted) || '';
}

function LinkTable({ title, fromLabel, toLabel, sources, targets, map, onChange, disabled }) {
  const rows = buildLinkRows(sources, map);
  return (
    <div className="space-y-1.5">
      <h4 className="text-[10px] uppercase font-bold text-slate-500">{title}</h4>
      {rows.length === 0 ? (
        <p className="text-[11px] text-slate-600">No {fromLabel} scenes yet.</p>
      ) : (
        <table className="w-full text-[11px]">
          <thead>
            <tr className="text-slate-500 text-left">
              <th className="font-medium pb-1 w-1/2">{fromLabel}</th>
              <th className="font-medium pb-1">{toLabel}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map(({ scene, target }) => (
              <tr key={scene}>
                <td className="py-0.5 pr-2 text-slate-300 truncate max-w-0">{scene}</td>
                <td className="py-0.5">
                  <select
                    aria-label={`${toLabel} scene for ${scene}`}
                    className={selectCls}
                    value={matchScene(targets, target)}
                    disabled={disabled}
                    onChange={(e) => onChange(setLinkEntry(map, scene, e.target.value))}
                  >
                    <option value="">— no link —</option>
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
  return (
    <section className="space-y-3">
      <label className="flex items-center gap-2 text-xs text-slate-300 cursor-pointer">
        <input
          type="checkbox"
          checked={link.enabled}
          disabled={busy}
          onChange={(e) => onChange({ ...link, enabled: e.target.checked })}
        />
        Scene link: keep Studio scenes and OBS scenes in step
      </label>
      <div className={`grid gap-4 sm:grid-cols-2 ${link.enabled ? '' : 'opacity-60'}`}>
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
      {obsScenes.length === 0 && (
        <p className="text-[10px] text-slate-600">Connect to OBS to see its scenes.</p>
      )}
    </section>
  );
}

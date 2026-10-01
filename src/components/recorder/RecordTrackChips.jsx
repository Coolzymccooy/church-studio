import React from 'react';

import { armedIndices } from '../../lib/recorder.js';

/**
 * Armed-track chips: one per mixer strip (click to arm/disarm), then the
 * Stream mix (always recorded) and Main mix (when included).
 */
export default function RecordTrackChips({ strips, armedStrips, includeMain, disabled, onToggle }) {
  const indices = strips.map((strip) => strip.index);
  const armed = new Set(armedIndices(armedStrips, indices));
  return (
    <div className="flex items-center gap-1 flex-wrap" role="group" aria-label="Tracks to record">
      {strips.map((strip) => {
        const on = armed.has(strip.index);
        return (
          <button
            key={strip.index}
            type="button"
            disabled={disabled}
            onClick={() => onToggle(strip.index)}
            aria-pressed={on}
            title={on ? `Recording ${strip.name} (raw). Click to skip it.` : `Not recording ${strip.name}. Click to arm it.`}
            className="px-1.5 py-0.5 rounded text-[9px] font-semibold border transition-colors disabled:opacity-50"
            style={on
              ? { background: 'rgba(255,82,82,0.15)', borderColor: 'rgba(255,82,82,0.5)', color: '#FF8A80' }
              : { background: 'transparent', borderColor: '#1e293b', color: '#64748b' }}
          >
            {String(strip.index + 1).padStart(2, '0')} {strip.name}
          </button>
        );
      })}
      <span
        className="px-1.5 py-0.5 rounded text-[9px] font-semibold border"
        style={{ background: 'rgba(var(--accent-rgb),0.12)', borderColor: 'rgba(var(--accent-rgb),0.4)', color: 'var(--accent)' }}
        title="The Stream mix is always recorded, exactly as it went to broadcast."
      >
        Stream Mix
      </span>
      {includeMain && (
        <span
          className="px-1.5 py-0.5 rounded text-[9px] font-semibold border"
          style={{ background: 'rgba(var(--accent-rgb),0.12)', borderColor: 'rgba(var(--accent-rgb),0.4)', color: 'var(--accent)' }}
        >
          Main Mix
        </span>
      )}
    </div>
  );
}

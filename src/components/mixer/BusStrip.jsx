import { faderDbToPosition, formatDb, positionToFaderDb } from '../../lib/mixerEngine.js';
import { LabeledSlider, VerticalFader } from './mixerControls.jsx';
import Meter from './Meter.jsx';

/**
 * BusStrip — Main / Stream / Monitor: fader, mute, limiter ceiling, L/R meter.
 */
export default function BusStrip({ bus, label, metersRef, onSetParam, onSetBool }) {
  const meter = () => metersRef.current.buses.find((b) => b.id === bus.id) || {};
  const faderPosition = faderDbToPosition(bus.fader_db);

  return (
    <div
      className="w-[132px] shrink-0 flex flex-col gap-2 rounded-lg border p-2"
      style={{ background: '#0D1428', borderColor: 'rgba(var(--accent-rgb),0.35)' }}
    >
      <div className="w-full px-1.5 py-1 rounded text-[10px] font-bold text-center tracking-wide uppercase" style={{ background: 'rgba(var(--accent-rgb),0.12)', color: 'var(--accent)' }}>
        {label}
      </div>

      <LabeledSlider
        label="Limiter" unit=" dB" min={-12} max={0} step={0.5}
        value={bus.limiter_ceiling_db}
        onChange={(v) => onSetParam('limiter_ceiling_db', v)}
        ariaLabel={`${label} limiter ceiling`}
      />

      <button
        type="button"
        onClick={() => onSetBool('mute', !bus.mute)}
        aria-pressed={bus.mute}
        className="w-full py-1 rounded text-[9px] font-bold border transition-all"
        style={bus.mute
          ? { background: '#FF5252', borderColor: '#FF5252', color: '#fff' }
          : { background: 'transparent', borderColor: '#1e293b', color: '#94a3b8' }}
      >
        MUTE
      </button>

      <div className="flex items-end justify-center gap-2 pt-1">
        <div className="flex flex-col items-center gap-1">
          <VerticalFader
            position={faderPosition}
            onChange={(p) => onSetParam('fader_db', positionToFaderDb(p))}
            ariaLabel={`${label} fader`}
            valueText={`${formatDb(bus.fader_db)} dB`}
          />
          <span className="text-[8px] font-mono text-slate-300">{formatDb(bus.fader_db)}</span>
        </div>
        <div className="flex flex-col items-center gap-1">
          <Meter
            bars={[
              { getDb: () => meter().peak_l_db ?? -96 },
              { getDb: () => meter().peak_r_db ?? -96 },
            ]}
            width={22}
            height={148}
          />
          <span className="text-[7px] font-mono text-slate-500">L&nbsp;&nbsp;R</span>
        </div>
      </div>
    </div>
  );
}

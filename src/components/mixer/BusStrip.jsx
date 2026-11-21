import { useCallback } from 'react';

import { faderDbToPosition, formatDb, positionToFaderDb } from '../../lib/mixerEngine.js';
import { BUS_LIMITER, busLabel } from '../../lib/faceplateStrip.js';
import { Fader, Key, Knob, MeterLadder, TwoPressKey } from '../faceplate/index.js';

const FADER_HEIGHT = 132;
const LADDER_HEIGHT = FADER_HEIGHT - 18;
const FADER_MARKS = [10, 0, -10, -30].map((db) => ({ at: faderDbToPosition(db), zero: db === 0 }));

/**
 * BusStrip — Main, Stream or a monitor mix (mon, mon 2, mon 3) on the faceplate: bus scribble, limiter ceiling knob (the
 * limiter is always on), mute (and, on mon 2 / mon 3, a two-press copy of the mon sends), and the
 * bus fader between its left and right peak ladders.
 */
export default function BusStrip({ bus, metersRef, onSetParam, onSetBool, onFaderTouch, onCopyFromMon1 = null }) {
  const label = busLabel(bus.id);
  const meter = useCallback(() => metersRef.current.buses.find((b) => b.id === bus.id) || {}, [metersRef, bus.id]);
  const leftDb = useCallback(() => meter().peak_l_db ?? -96, [meter]);
  const rightDb = useCallback(() => meter().peak_r_db ?? -96, [meter]);

  return (
    <div className="fp-strip" role="group" aria-label={`${label} bus`}>
      <div className={`fp-scrib fp-c-bus${bus.mute ? ' fp-dim' : ''}`}>
        <span className="fp-busname">{label}</span>
      </div>

      <div className="fp-knobs fp-knobs-1">
        <Knob
          label={BUS_LIMITER.label}
          job={BUS_LIMITER.job}
          min={BUS_LIMITER.min}
          max={BUS_LIMITER.max}
          step={BUS_LIMITER.step}
          defaultValue={BUS_LIMITER.defaultValue}
          value={bus.limiter_ceiling_db}
          valueText={BUS_LIMITER.fmt(bus.limiter_ceiling_db)}
          onChange={(v) => onSetParam('limiter_ceiling_db', v)}
        />
      </div>

      <div className="fp-krow" style={{ gridTemplateColumns: onCopyFromMon1 ? '1fr 1fr' : '1fr' }}>
        <Key lit={bus.mute ? 'amber' : null} pressed={bus.mute} onClick={() => onSetBool('mute', !bus.mute)} aria-label={`${label} mute`}>mute</Key>
        {onCopyFromMon1 && (
          <TwoPressKey label={`copy mon 1 sends into ${label}`} onConfirm={onCopyFromMon1}
            tip={`Press twice within 3 seconds. Every channel's ${label} send is set to its mon send, as a starting point.`}>
            copy
          </TwoPressKey>
        )}
      </div>

      <div className="fp-fz">
        <MeterLadder getDb={leftDb} height={LADDER_HEIGHT} width={5} label={`${label} left level`} />
        <Fader
          position={faderDbToPosition(bus.fader_db)}
          onTouch={onFaderTouch}
          onChange={(p) => onSetParam('fader_db', positionToFaderDb(p))}
          label={`${label} fader`}
          valueText={`${formatDb(bus.fader_db)} dB`}
          colour="bus"
          height={FADER_HEIGHT}
          marks={FADER_MARKS}
        />
        <MeterLadder getDb={rightDb} height={LADDER_HEIGHT} width={5} label={`${label} right level`} />
      </div>
      <span className="fp-fdb">{formatDb(bus.fader_db)}</span>
    </div>
  );
}

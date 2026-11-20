import { useCallback } from 'react';
import KeyPair from './KeyPair.jsx';

import { faderDbToPosition, formatDb, paramRange, positionToFaderDb } from '../../lib/mixerEngine.js';
import { STRIP_KNOBS } from '../../lib/faceplateStrip.js';
import { Fader, InfoTip, Key, Knob, MeterLadder } from '../faceplate/index.js';

const HEIGHT = 132;
const MARKS = [10, 0, -10, -30].map((db) => ({ at: faderDbToPosition(db), zero: db === 0 }));

function EffectKnob({ id, label, value, step, initial, unit, onSet }) {
  const [min, max] = paramRange(id);
  return <Knob label={label} job="dyn" size="s" min={min} max={max} step={step}
    value={value} defaultValue={initial} valueText={`${Number(value.toFixed(1))}${unit}`}
    onChange={(v) => onSet(id, v)} />;
}

export default function FxReturnStrip({ ret, index, extraMonitors, metersRef, onSet, onTap, onTempo }) {
  const a = index === 0;
  const label = a ? 'FX A' : 'FX B';
  const left = useCallback(() => metersRef.current.fxReturns?.[index]?.peak_l_db ?? -96, [metersRef, index]);
  const right = useCallback(() => metersRef.current.fxReturns?.[index]?.peak_r_db ?? -96, [metersRef, index]);
  const onChange = (key) => (value) => onSet(key, value);
  const toggle = (key, text, title, tone) => (
    <Key lit={ret[key] ? tone : null} pressed={ret[key]} aria-label={`${label} ${title}`}
      onClick={() => onSet(key, !ret[key])}>{text}</Key>
  );
  const sends = (keys) => (
    <div className={`fp-knobs fp-knobs-${keys.length}`}>
      {keys.map((name) => {
        const k = STRIP_KNOBS[name];
        const value = ret[k.param];
        return <Knob key={name} label={k.label} job={k.job} size="s" min={k.min} max={k.max} step={k.step}
          taper={k.taper} detent={k.detent} defaultValue={name === 'mon' ? -90 : k.defaultValue} value={value}
          valueText={value <= -90 ? 'Off' : k.fmt(value)} onChange={onChange(k.param)} />;
      })}
    </div>
  );

  return (
    <div className="fp-strip" role="group" aria-label={`${label} ${a ? 'reverb' : 'delay'} return`}>
      <div className={`fp-scrib fp-c-bus${ret.mute ? ' fp-dim' : ''}`}>
        <span className="fp-busname">{label} {a ? 'reverb' : 'delay'}</span>
      </div>
      {a ? (
        <>
          <div className="fp-knobs fp-knobs-1">
            <EffectKnob id="decay_s" label="decay" value={ret.decay_s} step={0.1} initial={1.8} unit="s" onSet={onSet} />
          </div>
          <div className="fp-keys h-6 items-center">
            <InfoTip label="Reverb decay">Time for low frequencies in the reverb tail to fall by 60 dB. Higher frequencies decay faster.</InfoTip>
          </div>
        </>
      ) : (
        <>
          <div className="fp-knobs fp-knobs-2">
            <EffectKnob id="tempo_bpm" label="tempo" value={ret.tempo_bpm} step={1} initial={120} unit="" onSet={(_, bpm) => onTempo(bpm)} />
            <EffectKnob id="feedback_pct" label="feedback" value={ret.feedback_pct} step={1} initial={25} unit="%" onSet={onSet} />
          </div>
          <div className="fp-keys items-center">
            <Key onClick={onTap}>tap</Key>
            <InfoTip label="Tap tempo">Tap quarter notes at the song tempo. Two taps set the tempo.</InfoTip>
          </div>
        </>
      )}
      {sends(['main', 'strm', 'mon'])}
      {extraMonitors.length > 0 && sends(extraMonitors.map((mon) => mon.knob))}
      <KeyPair>
        {toggle('monitor_post_fader', 'post', 'monitor send after fader', 'sel')}
        {toggle('solo', 'pfl', 'listen on monitor', 'sel')}
      </KeyPair>
      <KeyPair>
        {toggle('mute', 'mute', 'mute', 'amber')}
      </KeyPair>
      <div className="fp-fz">
        <MeterLadder getDb={left} height={HEIGHT - 18} width={5} label={`${label} left level`} />
        <Fader position={faderDbToPosition(ret.fader_db)} onChange={(p) => onSet('fader_db', positionToFaderDb(p))}
          label={`${label} fader`} valueText={`${formatDb(ret.fader_db)} dB`} colour="bus" height={HEIGHT} marks={MARKS} />
        <MeterLadder getDb={right} height={HEIGHT - 18} width={5} label={`${label} right level`} />
      </div>
      <span className="fp-fdb">{formatDb(ret.fader_db)}</span>
    </div>
  );
}

/**
 * Dev-only kit page for the faceplate primitives: open the dev server with ?kit.
 * It is not part of the production bundle (see main.jsx).
 */
import { useEffect, useRef, useState } from 'react';
import { faderDbToPosition, formatDb, positionToFaderDb } from '../../lib/mixerEngine.js';
import { Fader, Glass, InfoTip, Key, Knob, Lamp, MeterLadder, TwoPressKey } from './index.js';

const FINISHES = ['graphite', 'bone', 'auto'];
const MARKS = [10, 5, 0, -10, -20, -30, -50].map((db) => ({ at: faderDbToPosition(db), zero: db === 0 }));
const KNOBS = [
  { key: 'trim', label: 'trim', job: 'trim', min: -20, max: 40, step: 0.5, init: 12, fmt: (v) => `${v > 0 ? '+' : ''}${v}` },
  { key: 'hpf', label: 'hpf', job: 'hpf', min: 20, max: 400, step: 5, init: 80, fmt: (v) => `${v}` },
  { key: 'thr', label: 'g thr', job: 'dyn', min: -80, max: 0, step: 1, init: -45, fmt: (v) => `${v}` },
  { key: 'mid', label: 'mid', job: 'eq', min: -15, max: 15, step: 0.5, init: 2, bipolar: true, fmt: (v) => `${v > 0 ? '+' : ''}${v}` },
  { key: 'pan', label: 'pan', job: 'pan', min: -1, max: 1, step: 0.05, init: 0, bipolar: true, fmt: (v) => (v === 0 ? 'c' : `${v < 0 ? 'l' : 'r'}${Math.round(Math.abs(v) * 100)}`) },
  { key: 'strm', label: 'strm', job: 'send', min: -60, max: 10, step: 0.5, init: -6, fmt: (v) => `${v}` },
];

function useFakeSignal() {
  const db = useRef(-60);
  useEffect(() => {
    const start = performance.now();
    const id = setInterval(() => {
      const t = (performance.now() - start) / 1000;
      const speech = Math.abs(Math.sin(t * 7.3)) * Math.abs(Math.sin(t * 1.1));
      db.current = -50 + speech * 50 + (Math.sin(t * 0.3) > 0.9 ? 4 : 0);
    }, 30);
    return () => clearInterval(id);
  }, []);
  return () => db.current;
}

export default function FaceplateKit() {
  const [finish, setFinish] = useState('graphite');
  const [knobs, setKnobs] = useState(() => Object.fromEntries(KNOBS.map((k) => [k.key, k.init])));
  const [pos, setPos] = useState(faderDbToPosition(0));
  const [mute, setMute] = useState(false);
  const [log, setLog] = useState('none');
  const getDb = useFakeSignal();

  useEffect(() => {
    document.documentElement.dataset.finish = finish;
    return () => { delete document.documentElement.dataset.finish; };
  }, [finish]);

  const plate = { background: 'var(--fp-plate)', border: '1px solid var(--fp-seam)', borderRadius: 'var(--fp-radius-plate)', padding: 8, display: 'flex', flexDirection: 'column', gap: 8 };
  const row = { display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' };
  const title = { font: 'var(--fp-weight-semibold) var(--fp-text-md)/1.2 var(--fp-font-legend)', color: 'var(--fp-legend)' };

  return (
    <div style={{ minHeight: '100vh', background: 'var(--fp-chassis)', padding: 12, display: 'grid', gap: 8, gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))', alignContent: 'start', fontFamily: 'var(--fp-font-legend)' }}>
      <section style={plate} aria-label="Finish">
        <span style={title}>finish</span>
        <div style={row}>
          {FINISHES.map((f) => <Key key={f} lit={finish === f ? 'sel' : null} pressed={finish === f} onClick={() => setFinish(f)}>{f}</Key>)}
        </div>
      </section>

      <section style={plate} aria-label="Keys">
        <span style={title}>keys</span>
        <div style={row}>
          <Key>idle</Key><Key lit="green">running</Key><Key lit="amber">restart</Key>
          <Key lit="orange">voice</Key><Key lit="sel">bank 1</Key><Key lit="red">rec</Key><Key disabled>redo</Key>
          <Key lit={mute ? 'amber' : null} pressed={mute} onClick={() => setMute((m) => !m)}>mute</Key>
        </div>
        <div style={row}>
          <TwoPressKey label="load scene" onConfirm={() => setLog('scene loaded')} />
          <TwoPressKey label="stop engine" tone="red" onConfirm={() => setLog('engine stopped')} />
          <Lamp tone={log === 'none' ? 'off' : 'g'}>{log}</Lamp>
        </div>
      </section>

      <section style={plate} aria-label="Lamps and glass">
        <div style={{ ...row, justifyContent: 'space-between' }}>
          <span style={title}>lamps</span>
          <InfoTip label="lamps">Green is running, amber needs attention, red is recording or a fault.</InfoTip>
        </div>
        <div style={row}>
          <Lamp tone="g">running</Lamp><Lamp tone="a">restart to apply</Lamp><Lamp tone="r">rec</Lamp><Lamp>ndi in</Lamp>
        </div>
        <Glass style={{ padding: '5px 10px', fontSize: 'var(--fp-text-sm)', display: 'flex', gap: 14 }}>
          <span><span className="fp-glass-dim">sr </span>48.0k</span>
          <span><span className="fp-glass-dim">buf </span>256</span>
          <span><span className="fp-glass-dim">lat </span>5.3 ms</span>
        </Glass>
      </section>

      <section style={plate} aria-label="Knobs">
        <span style={title}>knobs</span>
        <div style={{ ...row, gap: 4 }}>
          {KNOBS.map((k) => (
            <Knob
              key={k.key}
              label={k.label}
              job={k.job}
              min={k.min}
              max={k.max}
              step={k.step}
              bipolar={k.bipolar}
              defaultValue={k.init}
              size={k.key === 'pan' || k.key === 'strm' ? 's' : 'l'}
              value={knobs[k.key]}
              valueText={k.fmt(knobs[k.key])}
              onChange={(v) => setKnobs((s) => ({ ...s, [k.key]: v }))}
            />
          ))}
        </div>
      </section>

      <section style={plate} aria-label="Fader and meters">
        <span style={title}>fader</span>
        <div style={{ ...row, alignItems: 'stretch', gap: 4 }}>
          <MeterLadder getDb={getDb} label="input level" />
          <Fader position={pos} onChange={setPos} label="strip 1 level" valueText={`${formatDb(positionToFaderDb(pos))} dB`} colour="amber" marks={MARKS} />
          <MeterLadder getDb={() => getDb() + 20 * Math.log10(Math.max(1e-6, 10 ** (positionToFaderDb(pos) / 20)))} label="output level" />
        </div>
        <span className="fp-fdb">{formatDb(positionToFaderDb(pos))}</span>
      </section>
    </div>
  );
}

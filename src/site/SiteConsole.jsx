import { useEffect, useState } from 'react';
import Fader from '../components/faceplate/Fader.jsx';
import Glass from '../components/faceplate/Glass.jsx';
import Key from '../components/faceplate/Key.jsx';
import Knob from '../components/faceplate/Knob.jsx';
import Lamp from '../components/faceplate/Lamp.jsx';
import MeterLadder from '../components/faceplate/MeterLadder.jsx';
import { ScaleContext } from '../components/faceplate/scaleContext.js';
import { demoLevelDb } from './demoSignal.js';

// A small working copy of the desk, built from the app's own faceplate parts, for the website.
const CHANNELS = [
  { name: 'pulpit', colour: 'amber', hi: 2, gate: -42, fader: 0.74 },
  { name: 'choir', colour: 'teal', hi: -1, gate: -50, fader: 0.66 },
  { name: 'keys', colour: 'olive', hi: 0, gate: -60, fader: 0.6 },
  { name: 'laptop', colour: 'sky', hi: 1, gate: -60, fader: 0.7 },
];
const FADER_HEIGHT = 112;
const MARKS = [{ at: 0.75, zero: true }, { at: 0.5 }, { at: 0.25 }];
const CLOCK_START_S = 42 * 60 + 17;
// On wide screens the strips are drawn larger, the way the mixer's size key does it.
const WIDE_QUERY = '(min-width: 1024px)';
const WIDE_ZOOM = 1.25;

const reducedMotion = () => typeof window !== 'undefined'
  && window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;

/** A meter source: moves like speech, or holds still when the visitor asks for reduced motion. */
function meterSource(seed) {
  const still = reducedMotion();
  const start = performance.now();
  return () => demoLevelDb(still ? 2 + seed : (performance.now() - start) / 1000, seed);
}

const faderDb = (p) => (p <= 0 ? '-inf' : (((p - 0.75) / 0.75) * 40).toFixed(1));
const signed = (v) => (v > 0 ? `+${v}` : String(v));
const clockText = (s) => [Math.floor(s / 3600), Math.floor(s / 60) % 60, s % 60]
  .map((n) => String(n).padStart(2, '0')).join(':');

function useClock() {
  const [s, setS] = useState(CLOCK_START_S);
  useEffect(() => {
    const id = setInterval(() => setS((v) => v + 1), 1000);
    return () => clearInterval(id);
  }, []);
  return clockText(s);
}

const wideQuery = () => (typeof window !== 'undefined' ? window.matchMedia?.(WIDE_QUERY) : undefined);

function useWideZoom() {
  const [wide, setWide] = useState(() => Boolean(wideQuery()?.matches));
  useEffect(() => {
    const mq = wideQuery();
    if (!mq) return undefined;
    const onChange = () => setWide(mq.matches);
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, []);
  return wide ? WIDE_ZOOM : 1;
}

function Strip({ ch, index }) {
  const [hi, setHi] = useState(ch.hi);
  const [gate, setGate] = useState(ch.gate);
  const [pos, setPos] = useState(ch.fader);
  const [muted, setMuted] = useState(false);
  const [getDb] = useState(() => meterSource(index));
  const db = faderDb(pos);
  return (
    <div className={`site-strip fp-c-${ch.colour}`} role="group" aria-label={ch.name}>
      <div className={`fp-scrib${muted ? ' fp-dim' : ''}`}>
        <span className="fp-no">{index + 1}</span>
        <span className="site-scrib-name">{ch.name}</span>
      </div>
      <div className="site-knobs">
        <Knob job="eq" size="s" label="hi" min={-12} max={12} step={0.5} bipolar
          value={hi} onChange={setHi} valueText={signed(hi)} />
        <Knob job="dyn" size="s" label="gate" min={-80} max={0} step={1}
          value={gate} onChange={setGate} valueText={String(gate)} />
      </div>
      <div className="fp-fz">
        <Fader position={pos} onChange={setPos} height={FADER_HEIGHT} marks={MARKS}
          colour={ch.colour} label={`${ch.name} level`} valueText={`${db} dB`} />
        <MeterLadder getDb={muted ? () => -90 : getDb} height={FADER_HEIGHT - 18} />
      </div>
      <span className="fp-fdb">{db}</span>
      <Key lit={muted ? 'amber' : null} pressed={muted} onClick={() => setMuted((m) => !m)}>mute</Key>
    </div>
  );
}

function Bus() {
  const [pos, setPos] = useState(0.75);
  const [getL] = useState(() => meterSource(7));
  const [getR] = useState(() => meterSource(8));
  return (
    <div className="site-strip site-bus fp-c-bus" role="group" aria-label="main">
      <div className="fp-scrib fp-c-bus"><span className="fp-busname">main</span></div>
      <Glass className="site-lufs"><span className="fp-glass-dim">lufs</span> -14.0</Glass>
      <div className="fp-fz">
        <Fader position={pos} onChange={setPos} height={FADER_HEIGHT} marks={MARKS}
          colour="bus" label="main level" valueText={`${faderDb(pos)} dB`} />
        <MeterLadder getDb={getL} height={FADER_HEIGHT - 18} />
        <MeterLadder getDb={getR} height={FADER_HEIGHT - 18} />
      </div>
      <span className="fp-fdb">{faderDb(pos)}</span>
      <Lamp tone="g">live</Lamp>
    </div>
  );
}

export default function SiteConsole() {
  const clock = useClock();
  const zoom = useWideZoom();
  return (
    <section className="site-console" aria-label="Demo desk">
      <div className="site-console-bar">
        <span className="fp-kl">demo desk</span>
        <span className="site-console-lamps">
          <Lamp tone="g">stream</Lamp>
          <Lamp tone="r">rec</Lamp>
          <Glass as="span" className="site-clock" role="timer" aria-label="Service time">{clock}</Glass>
        </span>
      </div>
      <ScaleContext.Provider value={zoom}>
        <div className="site-strips" style={{ zoom }}>
          {CHANNELS.map((ch, i) => <Strip key={ch.name} ch={ch} index={i} />)}
          <Bus />
        </div>
      </ScaleContext.Provider>
    </section>
  );
}

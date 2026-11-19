import { useEffect, useRef } from 'react';
import {
  HISTORY_S, LOUD_MAX, LOUD_MIN, ON_TARGET_LU, TOO_LOUD_LU, loudFraction, lufsText, pushHistory, targetDelta,
} from '../../lib/loudness.js';
import { readVisualiserPalette } from '../../lib/visualiserPalette.js';
import InfoTip from '../faceplate/InfoTip.jsx';
import Lamp from '../faceplate/Lamp.jsx';

const SAMPLE_MS = 250;
const SCALE = [-40, -30, -20, -10, 0];
const TONE_CLASS = { good: 'fp-good', caution: 'fp-caution', fault: 'fp-fault', low: '', idle: 'fp-glass-dim' };
const pct = (v) => `${(loudFraction(v) * 100).toFixed(2)}%`;

function drawHistory(canvas, history, target) {
  const ctx = canvas.getContext('2d');
  const dpr = window.devicePixelRatio || 1;
  const w = Math.max(1, canvas.clientWidth);
  const h = Math.max(1, canvas.clientHeight);
  if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  const cs = getComputedStyle(canvas);
  const p = readVisualiserPalette((name) => cs.getPropertyValue(name));
  const yOf = (v) => h - loudFraction(v) * h;
  ctx.fillStyle = p.bg;
  ctx.fillRect(0, 0, w, h);
  // target band and loud zone
  ctx.fillStyle = p.gDim;
  ctx.fillRect(0, yOf(target + ON_TARGET_LU), w, yOf(target - ON_TARGET_LU) - yOf(target + ON_TARGET_LU));
  // too loud for the platform: a faint zone with a red edge
  ctx.globalAlpha = 0.45;
  ctx.fillStyle = p.rDim;
  ctx.fillRect(0, 0, w, yOf(target + TOO_LOUD_LU));
  ctx.globalAlpha = 1;
  ctx.fillStyle = p.r;
  ctx.fillRect(0, Math.round(yOf(target + TOO_LOUD_LU)), w, 1);
  ctx.fillStyle = p.grid;
  [-30, -20, -10].forEach((v) => ctx.fillRect(0, Math.round(yOf(v)) + 0.5, w, 1));
  const max = Math.round((HISTORY_S * 1000) / SAMPLE_MS);
  const step = w / (max - 1);
  const x0 = w - (history.length - 1) * step;
  // filled area, then the line on top
  let open = false;
  ctx.beginPath();
  history.forEach((v, i) => {
    if (v === null) return;
    const x = x0 + i * step;
    if (!open) { ctx.moveTo(x, h); open = true; }
    ctx.lineTo(x, yOf(v));
  });
  if (open) {
    ctx.lineTo(w, h);
    ctx.closePath();
    ctx.fillStyle = p.grid;
    ctx.fill();
  }
  ctx.beginPath();
  let started = false;
  history.forEach((v, i) => {
    if (v === null) { started = false; return; }
    const x = x0 + i * step;
    if (!started) { ctx.moveTo(x, yOf(v)); started = true; } else ctx.lineTo(x, yOf(v));
  });
  ctx.strokeStyle = p.text;
  ctx.lineWidth = 1.5;
  ctx.lineJoin = 'round';
  ctx.stroke();
  ctx.strokeStyle = p.g;
  ctx.setLineDash([4, 4]);
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(0, Math.round(yOf(target)) + 0.5);
  ctx.lineTo(w, Math.round(yOf(target)) + 0.5);
  ctx.stroke();
  ctx.setLineDash([]);
}

function Gauge({ m, st, i, target, tone }) {
  return (
    <div>
      <div className="fp-gauge" role="img" aria-label={`short-term ${lufsText(st)} LUFS, target ${target}`}>
        <span className="fp-gauge-zone" style={{ left: pct(target - ON_TARGET_LU), width: `calc(${pct(target + ON_TARGET_LU)} - ${pct(target - ON_TARGET_LU)})` }} />
        <span className={`fp-gauge-fill fp-tone-${tone}`} style={{ width: pct(st) }} />
        {typeof m === 'number' && <span className="fp-gauge-tick" style={{ left: pct(m) }} />}
        {typeof i === 'number' && <span className="fp-gauge-int" style={{ left: pct(i) }} />}
      </div>
      <div className="fp-gauge-scale">
        {SCALE.map((v) => <span key={v} style={{ left: pct(v) }}>{v}</span>)}
      </div>
    </div>
  );
}

/**
 * The loudness module: short-term LUFS as the big reading, distance to the platform target, a
 * gauge with the target zone, momentary and integrated marks, and a 60-second history.
 */
export default function LoudnessPanel({ m, st, i, target, limiterOn }) {
  const canvasRef = useRef(null);
  const latest = useRef({ st, target });
  const history = useRef([]);
  useEffect(() => {
    latest.current = { st, target };
  });

  useEffect(() => {
    const max = Math.round((HISTORY_S * 1000) / SAMPLE_MS);
    const tick = () => {
      history.current = pushHistory(history.current, latest.current.st, max);
      if (canvasRef.current) drawHistory(canvasRef.current, history.current, latest.current.target);
    };
    tick();
    const id = setInterval(tick, SAMPLE_MS);
    return () => clearInterval(id);
  }, []);

  const delta = targetDelta(st, target);
  return (
    <aside className="fp-loud" aria-label="Loudness">
      <div className="fp-mod-hd" style={{ padding: '0 0 8px' }}>
        <span>
          Loudness
          <InfoTip label="Loudness">Short-term loudness of the stream (EBU R128 / ITU BS.1770), against the platform target set in profiles.</InfoTip>
        </span>
        <Lamp tone={limiterOn ? 'g' : 'off'}>{limiterOn ? 'limiter on' : 'limiter off'}</Lamp>
      </div>
      <div className="fp-glass fp-loud-read">
        <div className="fp-loud-big"><span className="fp-num">{lufsText(st)}</span><small>LUFS</small></div>
        <div className={`fp-num ${TONE_CLASS[delta.tone]}`}>{delta.text}</div>
      </div>
      <Gauge m={m} st={st} i={i} target={target} tone={delta.tone} />
      <canvas ref={canvasRef} className="fp-loud-hist" role="img" aria-label={`Loudness over the last ${HISTORY_S} seconds`} />
      <div className="fp-row"><span className="fp-sub">momentary</span><span className="fp-num">{lufsText(m)}</span></div>
      <div className="fp-row"><span className="fp-sub">integrated</span><span className="fp-num">{lufsText(i)}</span></div>
      <div className="fp-row"><span className="fp-sub">target</span><span className="fp-num">{target} LUFS</span></div>
      <div className="fp-row fp-sub"><span>{LOUD_MIN} to {LOUD_MAX} LUFS · {HISTORY_S} s</span></div>
    </aside>
  );
}

import { useEffect, useRef } from 'react';
import { initialMeter, meterFraction, meterZone, stepMeter, ZONE_AMBER_DB, ZONE_RED_DB } from '../../lib/faceplateMeter.js';

const SEG = 3;
const GAP = 1;
const ZONES = [
  { key: 'g', from: 0, to: meterFraction(ZONE_AMBER_DB), lit: '--fp-led-g', dim: '--fp-led-g-dim' },
  { key: 'a', from: meterFraction(ZONE_AMBER_DB), to: meterFraction(ZONE_RED_DB), lit: '--fp-led-a', dim: '--fp-led-a-dim' },
  { key: 'r', from: meterFraction(ZONE_RED_DB), to: 1, lit: '--fp-led-r', dim: '--fp-led-r-dim' },
];

function readColours(el) {
  const cs = getComputedStyle(el);
  const get = (name) => cs.getPropertyValue(name).trim();
  return { bg: get('--fp-meter-bg'), zones: ZONES.map((z) => ({ ...z, lit: get(z.lit), dim: get(z.dim) })) };
}

function drawBar(ctx, w, h, colours, meter) {
  ctx.fillStyle = colours.bg;
  ctx.fillRect(0, 0, w, h);
  const level = meterFraction(meter.level) * w;
  colours.zones.forEach((z) => {
    const x0 = z.from * w;
    const x1 = z.to * w;
    ctx.fillStyle = z.dim;
    ctx.fillRect(x0, 0, x1 - x0, h);
    const litEnd = Math.min(x1, level);
    if (litEnd > x0) {
      ctx.fillStyle = z.lit;
      ctx.fillRect(x0, 0, litEnd - x0, h);
    }
  });
  ctx.fillStyle = colours.bg;
  for (let x = SEG; x < w; x += SEG + GAP) ctx.fillRect(x, 0, GAP, h);
  if (meter.peak > -60) {
    const zone = colours.zones.find((z) => z.key === meterZone(meter.peak));
    ctx.fillStyle = zone.lit;
    ctx.fillRect(Math.min(w - 2, meterFraction(meter.peak) * w), 0, 2, h);
  }
}

/**
 * A slim horizontal LED meter (green to -18 dBFS, amber to -3, red above, peak hold) that fills its
 * container's width. `getDb` is read every animation frame, so levels never re-render React.
 */
export default function LevelBar({ getDb, height = 6, label }) {
  const canvasRef = useRef(null);
  const getDbRef = useRef(getDb);
  useEffect(() => {
    getDbRef.current = getDb;
  });

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return undefined;
    const ctx = canvas.getContext('2d');
    const colours = readColours(canvas);
    let meter = initialMeter();
    let last = performance.now();
    let raf = 0;
    const frame = (now) => {
      const dpr = window.devicePixelRatio || 1;
      const w = Math.max(1, canvas.clientWidth);
      if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(height * dpr)) {
        canvas.width = Math.round(w * dpr);
        canvas.height = Math.round(height * dpr);
      }
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      const dt = Math.min(0.1, Math.max(0, (now - last) / 1000));
      last = now;
      meter = stepMeter(meter, getDbRef.current(), dt);
      drawBar(ctx, w, height, colours, meter);
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  }, [height]);

  return (
    <canvas
      ref={canvasRef}
      className="fp-ladder"
      style={{ width: '100%', height }}
      role={label ? 'img' : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
    />
  );
}

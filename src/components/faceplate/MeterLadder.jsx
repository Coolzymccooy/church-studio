import { useEffect, useRef } from 'react';
import { initialMeter, meterFraction, meterZone, stepMeter, ZONE_AMBER_DB, ZONE_RED_DB } from '../../lib/faceplateMeter.js';

const SEGMENT_PX = 4;
const GAP_PX = 1;
const ZONES = [
  { key: 'g', from: 0, to: meterFraction(ZONE_AMBER_DB), lit: '--fp-led-g', dim: '--fp-led-g-dim' },
  { key: 'a', from: meterFraction(ZONE_AMBER_DB), to: meterFraction(ZONE_RED_DB), lit: '--fp-led-a', dim: '--fp-led-a-dim' },
  { key: 'r', from: meterFraction(ZONE_RED_DB), to: 1, lit: '--fp-led-r', dim: '--fp-led-r-dim' },
];

function readColours(el) {
  const cs = getComputedStyle(el);
  const get = (name) => cs.getPropertyValue(name).trim();
  return {
    bg: get('--fp-meter-bg'),
    zones: ZONES.map((z) => ({ ...z, lit: get(z.lit), dim: get(z.dim) })),
  };
}

function drawLadder(ctx, w, h, colours, meter) {
  ctx.fillStyle = colours.bg;
  ctx.fillRect(0, 0, w, h);
  const level = meterFraction(meter.level) * h;
  colours.zones.forEach((z) => {
    const y0 = h - z.to * h;
    const y1 = h - z.from * h;
    ctx.fillStyle = z.dim;
    ctx.fillRect(0, y0, w, y1 - y0);
    const litTop = Math.max(y0, h - level);
    if (litTop < y1) {
      ctx.fillStyle = z.lit;
      ctx.fillRect(0, litTop, w, y1 - litTop);
    }
  });
  ctx.fillStyle = colours.bg;
  for (let y = h - SEGMENT_PX; y > 0; y -= SEGMENT_PX) ctx.fillRect(0, y, w, GAP_PX);
  if (meter.peak > -60) {
    const zone = colours.zones.find((z) => z.key === meterZone(meter.peak));
    ctx.fillStyle = zone.lit;
    ctx.fillRect(0, Math.max(0, h - meterFraction(meter.peak) * h - 1), w, 2);
  }
}

/**
 * A segmented meter ladder: green to -18 dBFS, amber to -3, red above, with a one-second peak hold.
 * `getDb` is read every animation frame, so live meter updates never re-render React.
 */
export default function MeterLadder({ getDb, height = 178, width = 6, label }) {
  const canvasRef = useRef(null);
  const getDbRef = useRef(getDb);
  useEffect(() => {
    getDbRef.current = getDb;
  });

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return undefined;
    const ctx = canvas.getContext('2d');
    const dpr = window.devicePixelRatio || 1;
    canvas.width = width * dpr;
    canvas.height = height * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const colours = readColours(canvas);
    let meter = initialMeter();
    let last = performance.now();
    let raf = 0;
    const frame = (now) => {
      const dt = Math.min(0.1, Math.max(0, (now - last) / 1000));
      last = now;
      meter = stepMeter(meter, getDbRef.current(), dt);
      drawLadder(ctx, width, height, colours, meter);
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  }, [width, height]);

  return (
    <canvas
      ref={canvasRef}
      className="fp-ladder"
      style={{ width, height }}
      role={label ? 'img' : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
    />
  );
}

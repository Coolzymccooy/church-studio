import { useEffect, useRef } from 'react';

/**
 * Meter — canvas level meter with smooth decay and peak hold, driven by
 * requestAnimationFrame. `bars` is an array of { getDb } accessors read fresh
 * every frame; nothing here touches React state, so 20 Hz mixer-meters
 * updates never trigger a re-render of the console.
 */
const MIN_DB = -60;
const MAX_DB = 6;
const PEAK_HOLD_SECONDS = 1;
const PEAK_DECAY_DB_PER_SEC = 20;

function dbToFraction(db) {
  const clamped = Math.max(MIN_DB, Math.min(MAX_DB, db));
  return (clamped - MIN_DB) / (MAX_DB - MIN_DB);
}

function colorForFraction(fraction) {
  if (fraction > 0.92) return '#FF5252';
  if (fraction > 0.78) return '#FFB800';
  return '#00E676';
}

export default function Meter({ bars, width = 30, height = 96, className = '' }) {
  const canvasRef = useRef(null);
  const barsRef = useRef(bars);
  useEffect(() => {
    barsRef.current = bars;
  });

  const smoothedRef = useRef(bars.map(() => MIN_DB));
  const peakRef = useRef(bars.map(() => MIN_DB));
  const peakHoldRef = useRef(bars.map(() => 0));

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return undefined;
    const ctx = canvas.getContext('2d');
    const dpr = window.devicePixelRatio || 1;
    canvas.width = width * dpr;
    canvas.height = height * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    let rafId = null;
    let lastTime = performance.now();

    const draw = (time) => {
      const dt = Math.min(0.1, Math.max(0, (time - lastTime) / 1000));
      lastTime = time;
      const activeBars = barsRef.current;
      ctx.clearRect(0, 0, width, height);

      const gap = activeBars.length > 1 ? 3 : 0;
      const barWidth = (width - gap * (activeBars.length - 1)) / activeBars.length;

      activeBars.forEach((bar, i) => {
        const target = Number.isFinite(bar.getDb()) ? bar.getDb() : MIN_DB;
        const previous = smoothedRef.current[i] ?? MIN_DB;
        const rate = target > previous ? 22 : 7;
        const next = previous + (target - previous) * Math.min(1, rate * dt);
        smoothedRef.current[i] = next;

        if (next >= (peakRef.current[i] ?? MIN_DB)) {
          peakRef.current[i] = next;
          peakHoldRef.current[i] = PEAK_HOLD_SECONDS;
        } else {
          peakHoldRef.current[i] = (peakHoldRef.current[i] ?? 0) - dt;
          if (peakHoldRef.current[i] <= 0) {
            peakRef.current[i] = Math.max(MIN_DB, (peakRef.current[i] ?? MIN_DB) - dt * PEAK_DECAY_DB_PER_SEC);
          }
        }

        const x = i * (barWidth + gap);
        const frac = dbToFraction(next);
        const barHeight = Math.max(1, frac * height);
        ctx.fillStyle = colorForFraction(frac);
        ctx.fillRect(x, height - barHeight, barWidth, barHeight);

        const peakFrac = dbToFraction(peakRef.current[i] ?? MIN_DB);
        ctx.fillStyle = '#e2e8f0';
        ctx.fillRect(x, Math.max(0, height - peakFrac * height - 1.5), barWidth, 1.5);
      });

      rafId = requestAnimationFrame(draw);
    };

    rafId = requestAnimationFrame(draw);
    return () => {
      if (rafId) cancelAnimationFrame(rafId);
    };
  }, [width, height]);

  return (
    <canvas
      ref={canvasRef}
      style={{ width, height, display: 'block' }}
      className={className}
      aria-hidden="true"
    />
  );
}

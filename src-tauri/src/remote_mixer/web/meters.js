// Segmented meter ladders on canvas, all driven by one animation-frame loop. Each ladder has
// one or more lanes; each lane draws a bar (ballistic, with a held peak tick), an optional
// ghost (signal at the source that the mix is not using) and an optional peak dot.
// Reduced motion: readings are drawn as they arrive, with no smoothing and no sweep.
import { initialMeter, meterFraction, stepMeter, ZONE_AMBER_DB, ZONE_RED_DB, METER_MIN_DB } from './mixmath.js';

const SEGMENT_PX = 3;
const GAP_PX = 1;
const LANE_GAP_PX = 4;
const IDLE_AFTER_MS = 1500;
const SWEEP_MS = 760;
const SWEEP_STAGGER_MS = 28;
const ZONES = [
  { from: 0, to: meterFraction(ZONE_AMBER_DB), lit: '--fp-led-g', dim: '--fp-led-g-dim' },
  { from: meterFraction(ZONE_AMBER_DB), to: meterFraction(ZONE_RED_DB), lit: '--fp-led-a', dim: '--fp-led-a-dim' },
  { from: meterFraction(ZONE_RED_DB), to: 1, lit: '--fp-led-r', dim: '--fp-led-r-dim' },
];
const reduced = window.matchMedia('(prefers-reduced-motion: reduce)');

function palette() {
  const style = getComputedStyle(document.documentElement);
  const read = (name) => style.getPropertyValue(name).trim();
  return {
    bg: read('--fp-meter-bg'),
    ghost: read('--fp-glass-edge'),
    dot: read('--fp-glass-text'),
    zones: ZONES.map((zone) => ({ ...zone, lit: read(zone.lit), dim: read(zone.dim) })),
  };
}

function zoneColour(colours, fraction) {
  return (colours.zones.find((zone) => fraction < zone.to) ?? colours.zones[2]).lit;
}

function freshLane() {
  return { bar: initialMeter(), dot: initialMeter(), ghost: initialMeter() };
}

function sizeCanvas(ladder) {
  const box = ladder.canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  ladder.w = box.width;
  ladder.h = box.height;
  ladder.canvas.width = Math.max(1, Math.round(box.width * dpr));
  ladder.canvas.height = Math.max(1, Math.round(box.height * dpr));
  ladder.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
}

/** Fill the span [p0, p1] of the meter's travel within one lane. */
function span(ctx, ladder, lane, p0, p1, colour) {
  if (p1 <= p0) return;
  ctx.fillStyle = colour;
  if (ladder.vertical) {
    ctx.fillRect(lane.at, ladder.h - p1 * ladder.h, lane.size, (p1 - p0) * ladder.h);
  } else {
    ctx.fillRect(p0 * ladder.w, lane.at, (p1 - p0) * ladder.w, lane.size);
  }
}

function lanesOf(ladder) {
  const across = ladder.vertical ? ladder.w : ladder.h;
  const count = ladder.lanes.length;
  const size = (across - LANE_GAP_PX * (count - 1)) / count;
  return ladder.lanes.map((_, i) => ({ at: i * (size + LANE_GAP_PX), size }));
}

function drawLane(ctx, ladder, lane, state, target, colours) {
  for (const zone of colours.zones) span(ctx, ladder, lane, zone.from, zone.to, zone.dim);
  if (target.ghost !== undefined) span(ctx, ladder, lane, 0, meterFraction(state.ghost.level), colours.ghost);
  const level = meterFraction(state.bar.level);
  for (const zone of colours.zones) span(ctx, ladder, lane, zone.from, Math.min(zone.to, level), zone.lit);
  const tick = (fraction, colour) => {
    const step = 2 / (ladder.vertical ? ladder.h : ladder.w);
    span(ctx, ladder, lane, Math.max(0, fraction - step), fraction, colour);
  };
  const held = target.dot !== undefined ? state.dot.peak : state.bar.peak;
  if (held > METER_MIN_DB) tick(meterFraction(held), zoneColour(colours, meterFraction(held)));
  if (target.dot !== undefined && state.dot.level > METER_MIN_DB) tick(meterFraction(state.dot.level), colours.dot);
  if (target.clip) span(ctx, ladder, lane, meterFraction(ZONE_RED_DB) + 0.04, 1, colours.zones[2].lit);
}

function drawGaps(ctx, ladder, colour) {
  ctx.fillStyle = colour;
  if (ladder.vertical) {
    for (let y = ladder.h - SEGMENT_PX; y > 0; y -= SEGMENT_PX + GAP_PX) ctx.fillRect(0, y - GAP_PX, ladder.w, GAP_PX);
  } else {
    for (let x = SEGMENT_PX; x < ladder.w; x += SEGMENT_PX + GAP_PX) ctx.fillRect(x, 0, GAP_PX, ladder.h);
  }
}

export function createMeters() {
  const ladders = new Set();
  let colours = palette();
  let frame = 0;
  let last = 0;
  let activeUntil = 0;
  let sweepFrom = 0;

  // Resize on the next frame, so canvas sizing never re-enters the observer's own pass.
  const observer = new ResizeObserver((entries) => {
    const targets = new Set(entries.map((entry) => entry.target));
    requestAnimationFrame(() => {
      for (const ladder of ladders) if (targets.has(ladder.canvas)) sizeCanvas(ladder);
      wake();
    });
  });

  function sweepTarget(ladder, now) {
    if (!sweepFrom) return null;
    const t = (now - sweepFrom - ladder.order * SWEEP_STAGGER_MS) / SWEEP_MS;
    if (t <= 0) return METER_MIN_DB;
    if (t >= 1) return null;
    return METER_MIN_DB + (6 - METER_MIN_DB) * Math.sin(Math.PI * t);
  }

  function draw(now) {
    const dt = Math.min(0.1, Math.max(0, (now - last) / 1000));
    last = now;
    const instant = reduced.matches;
    let sweeping = false;
    for (const ladder of ladders) {
      if (!ladder.w || !ladder.h) continue;
      const sweep = instant ? null : sweepTarget(ladder, now);
      if (sweep !== null) sweeping = true;
      const targets = sweep === null ? ladder.read() : ladder.lanes.map(() => ({ bar: sweep }));
      const { ctx } = ladder;
      ctx.fillStyle = colours.bg;
      ctx.fillRect(0, 0, ladder.w, ladder.h);
      lanesOf(ladder).forEach((lane, i) => {
        const target = targets[i] ?? { bar: -Infinity };
        const state = ladder.lanes[i];
        state.bar = stepMeter(state.bar, target.bar, dt, { instant });
        state.dot = stepMeter(state.dot, target.dot ?? -Infinity, dt, { instant: true });
        state.ghost = stepMeter(state.ghost, target.ghost ?? -Infinity, dt, { instant });
        drawLane(ctx, ladder, lane, state, target, colours);
      });
      drawGaps(ctx, ladder, colours.bg);
    }
    if (sweepFrom && !sweeping && now - sweepFrom > SWEEP_MS) {
      sweepFrom = 0;
      for (const ladder of ladders) ladder.lanes = ladder.lanes.map(freshLane);
    }
    frame = now < activeUntil || sweeping ? requestAnimationFrame(draw) : 0;
  }

  function wake() {
    activeUntil = performance.now() + IDLE_AFTER_MS;
    if (!frame) {
      last = performance.now();
      frame = requestAnimationFrame(draw);
    }
  }

  return {
    /** Add a ladder. `read()` returns one target per lane: { bar, dot?, ghost?, clip? } in dB. */
    add(canvas, { vertical = true, lanes = 1, read, order = 0 }) {
      const ladder = { canvas, vertical, read, order, ctx: canvas.getContext('2d'), lanes: Array.from({ length: lanes }, freshLane), w: 0, h: 0 };
      ladders.add(ladder);
      sizeCanvas(ladder);
      observer.observe(canvas);
      wake();
      return () => {
        ladders.delete(ladder);
        observer.unobserve(canvas);
      };
    },
    /** New readings arrived: keep drawing for a while. */
    wake,
    /** The finish changed: read the lamp colours again. */
    refresh() {
      colours = palette();
      wake();
    },
    /** A one-shot power-on sweep, like a desk's lamp test. Skipped under reduced motion. */
    sweep() {
      if (reduced.matches) return;
      sweepFrom = performance.now();
      wake();
    },
  };
}

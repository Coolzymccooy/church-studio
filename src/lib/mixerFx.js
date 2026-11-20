const OFF = -90;
const LEVEL = [OFF, 10];
const RETURN_LEVELS = {
  fader_db: LEVEL,
  send_main_db: LEVEL,
  send_stream_db: LEVEL,
  send_monitor_db: LEVEL,
  send_monitor2_db: LEVEL,
  send_monitor3_db: LEVEL,
};

export const FX_RANGES = {
  send_fx_a_db: LEVEL,
  send_fx_b_db: LEVEL,
  decay_s: [0.3, 5],
  tempo_bpm: [40, 240],
  feedback_pct: [0, 80],
};

const EFFECT_KEYS = { fx_a: ['decay_s'], fx_b: ['tempo_bpm', 'feedback_pct'] };
const RETURN_FLAGS = ['mute', 'solo', 'monitor_post_fader'];
const RETURN_DEFAULTS = {
  fader_db: 0,
  mute: false,
  solo: false,
  monitor_post_fader: false,
  send_main_db: 0,
  send_stream_db: 0,
  send_monitor_db: OFF,
  send_monitor2_db: OFF,
  send_monitor3_db: OFF,
};

export function defaultFxReturns() {
  return [
    { ...RETURN_DEFAULTS, id: 'fx_a', decay_s: 1.8 },
    { ...RETURN_DEFAULTS, id: 'fx_b', tempo_bpm: 120, feedback_pct: 25 },
  ];
}

export function fxParamRange(id, key) {
  if (!Object.hasOwn(EFFECT_KEYS, id)) return null;
  if (Object.hasOwn(RETURN_LEVELS, key)) return RETURN_LEVELS[key];
  return EFFECT_KEYS[id].includes(key) ? FX_RANGES[key] : null;
}

export function isFxFlag(key) {
  return RETURN_FLAGS.includes(key);
}

export function applyFxChange(state, id, key, value) {
  const fxReturns = state.fxReturns.map((ret) => (ret.id === id ? { ...ret, [key]: value } : ret));
  return { ...state, fxReturns };
}

export function createTapTempo() {
  let previous = null;
  let intervals = [];
  const tap = (now) => {
    const interval = previous === null ? 0 : now - previous;
    previous = now;
    if (interval < 250 || interval > 1500) {
      intervals = [];
      return null;
    }
    intervals = [...intervals.slice(-3), interval];
    const sorted = [...intervals].sort((a, b) => a - b);
    const mid = Math.floor(sorted.length / 2);
    const median = (sorted[mid] + sorted[Math.floor((sorted.length - 1) / 2)]) / 2;
    return Math.round(60_000 / median);
  };
  tap.reset = () => {
    previous = null;
    intervals = [];
  };
  return tap;
}

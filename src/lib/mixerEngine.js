/**
 * mixerEngine.js — UI-side mixer controller, matching
 * docs/specs/2026-09-27-mixer-live-contract.md exactly:
 *   - Tauri command names, argument shapes and the MixerState/meters JSON.
 *
 * Exposes two factories that implement the same interface:
 *   - createMixerController({ invoke, listen })  — thin wrapper around the real
 *     Tauri commands, used when running inside the desktop app.
 *   - createMockMixerController()                — fully in-memory implementation
 *     used for the browser demo mixer.
 *
 * Also exposes pure, unit-tested helpers used by the mixer UI:
 *   faderDbToPosition / positionToFaderDb, formatDb, clampStripValue,
 *   validateSceneName and the applyStripChange reducer.
 */

// ── Fader law ────────────────────────────────────────────────────────────────
// Two-segment piecewise-linear law: −90 dB at the very bottom (position 0),
// 0 dB at 75% of travel, and +10 dB at the very top (position 1). Each segment
// is linear in dB-per-position, which keeps the law exactly invertible and
// gives faders the familiar "long throw below 0 dB, short boost above it" feel
// without needing a lookup table.
const FADER_MIN_DB = -90;
const FADER_MAX_DB = 10;
const FADER_ZERO_POSITION = 0.75;

export function positionToFaderDb(position) {
  const p = Math.min(1, Math.max(0, position));
  if (p <= FADER_ZERO_POSITION) {
    return FADER_MIN_DB + (p / FADER_ZERO_POSITION) * (0 - FADER_MIN_DB);
  }
  const upperSpan = 1 - FADER_ZERO_POSITION;
  return ((p - FADER_ZERO_POSITION) / upperSpan) * FADER_MAX_DB;
}

export function faderDbToPosition(db) {
  const clamped = Math.min(FADER_MAX_DB, Math.max(FADER_MIN_DB, db));
  if (clamped <= 0) {
    return ((clamped - FADER_MIN_DB) / (0 - FADER_MIN_DB)) * FADER_ZERO_POSITION;
  }
  const upperSpan = 1 - FADER_ZERO_POSITION;
  return FADER_ZERO_POSITION + (clamped / FADER_MAX_DB) * upperSpan;
}

export function formatDb(db) {
  if (!Number.isFinite(db) || db <= FADER_MIN_DB) return '-inf';
  const sign = db >= 0 ? '+' : '';
  return `${sign}${db.toFixed(1)}`;
}

// ── Contract ranges ──────────────────────────────────────────────────────────
const PARAM_RANGES = {
  trim_db: [-20, 40],
  hpf_hz: [20, 400],
  gate_threshold_db: [-80, 0],
  eq_low_db: [-15, 15],
  eq_mid_db: [-15, 15],
  eq_high_db: [-15, 15],
  comp_threshold_db: [-40, 0],
  comp_ratio: [1, 20],
  pan: [-1, 1],
  fader_db: [FADER_MIN_DB, FADER_MAX_DB],
  send_main_db: [FADER_MIN_DB, FADER_MAX_DB],
  send_stream_db: [FADER_MIN_DB, FADER_MAX_DB],
  send_monitor_db: [FADER_MIN_DB, FADER_MAX_DB],
  limiter_ceiling_db: [-12, 0],
};

const STRIP_BOOL_KEYS = new Set([
  'polarity',
  'hpf_enabled',
  'gate_enabled',
  'comp_enabled',
  'mute',
  'solo',
  'monitor_post_fader',
  'voice_chain',
]);

export function clampStripValue(key, value) {
  const range = PARAM_RANGES[key];
  if (!range) return value;
  const [min, max] = range;
  const numeric = Number.isFinite(value) ? value : min;
  return Math.min(max, Math.max(min, numeric));
}

export function validateSceneName(name) {
  if (typeof name !== 'string' || name.length === 0) {
    return 'Scene name is required.';
  }
  if (name.length > 40) {
    return 'Scene name must be 40 characters or fewer.';
  }
  if (!/^[A-Za-z0-9 _-]+$/.test(name)) {
    return 'Scene name can only contain letters, digits, spaces, - and _.';
  }
  return null;
}

// ── Immutable strip-change reducer ──────────────────────────────────────────
/**
 * Returns a new MixerState with strip[index][key] set to value, clamped per
 * the contract ranges. Enforces voice_chain exclusivity: turning it on for one
 * strip turns it off for every other strip.
 */
export function applyStripChange(state, index, key, value) {
  const isVoiceChainOn = key === 'voice_chain' && Boolean(value) === true;
  const strips = state.strips.map((strip, i) => {
    if (i !== index) {
      if (isVoiceChainOn && strip.voice_chain) {
        return { ...strip, voice_chain: false };
      }
      return strip;
    }
    const nextValue = STRIP_BOOL_KEYS.has(key) ? Boolean(value) : clampStripValue(key, value);
    return { ...strip, [key]: nextValue };
  });
  return { ...state, strips };
}

// ── Real controller — thin wrapper over the Tauri commands ─────────────────
export function createMixerController({ invoke, listen }) {
  return {
    getState() {
      return invoke('mixer_state');
    },
    setStripParam(index, key, value) {
      return invoke('mixer_set_strip_param', { index, key, value });
    },
    setStripBool(index, key, value) {
      return invoke('mixer_set_strip_bool', { index, key, value });
    },
    renameStrip(index, name) {
      return invoke('mixer_rename_strip', { index, name });
    },
    setBusParam(bus, key, value) {
      return invoke('mixer_set_bus_param', { bus, key, value });
    },
    setBusBool(bus, key, value) {
      return invoke('mixer_set_bus_bool', { bus, key, value });
    },
    listScenes() {
      return invoke('mixer_list_scenes');
    },
    saveScene(name) {
      return invoke('mixer_save_scene', { name });
    },
    loadScene(name) {
      return invoke('mixer_load_scene', { name });
    },
    deleteScene(name) {
      return invoke('mixer_delete_scene', { name });
    },
    subscribeMeters(cb) {
      let disposed = false;
      let unlisten = null;
      const ready = listen('mixer-meters', (event) => {
        if (!disposed) cb(event.payload);
      }).then((fn) => {
        if (disposed) {
          fn();
          return null;
        }
        unlisten = fn;
        return fn;
      });
      return {
        ready,
        dispose() {
          disposed = true;
          if (unlisten) {
            unlisten();
            unlisten = null;
          }
        },
      };
    },
  };
}

// ── Mock controller — fully in-memory, for the browser demo ────────────────
const MOCK_STRIP_NAMES = [
  'Pastor',
  'Lectern',
  'Choir L',
  'Choir R',
  'Keys',
  'Bass',
  'Playback L',
  'Playback R',
];

function defaultStrip(index, name, overrides = {}) {
  return {
    index,
    name,
    trim_db: 0,
    polarity: false,
    hpf_enabled: true,
    hpf_hz: 80,
    gate_enabled: false,
    gate_threshold_db: -45,
    eq_low_db: 0,
    eq_mid_db: 0,
    eq_high_db: 0,
    comp_enabled: false,
    comp_threshold_db: -18,
    comp_ratio: 3,
    pan: 0,
    mute: false,
    solo: false,
    fader_db: 0,
    send_main_db: 0,
    send_stream_db: 0,
    send_monitor_db: 0,
    monitor_post_fader: false,
    voice_chain: false,
    ...overrides,
  };
}

function defaultBus(id) {
  return { id, fader_db: 0, mute: false, limiter_ceiling_db: -1 };
}

function createInitialMockState() {
  return {
    inputChannels: MOCK_STRIP_NAMES.length,
    running: true,
    strips: MOCK_STRIP_NAMES.map((name, index) =>
      defaultStrip(index, name, index === 0 ? { voice_chain: true } : {})),
    buses: ['main', 'stream', 'monitor'].map(defaultBus),
  };
}

function cloneState(state) {
  return {
    ...state,
    strips: state.strips.map((strip) => ({ ...strip })),
    buses: state.buses.map((bus) => ({ ...bus })),
  };
}

function clampMeterDb(value) {
  return Math.max(-96, Math.min(6, value));
}

function round1(value) {
  return Math.round(value * 10) / 10;
}

function buildMockMetersPayload(state, tick) {
  const strips = state.strips.map((strip, i) => {
    const pre = clampMeterDb(-18 + Math.sin(tick * 0.3 + i * 1.7) * 6 + (Math.random() - 0.5) * 3);
    const post = strip.mute ? -96 : clampMeterDb(pre + strip.fader_db);
    const gateOpen = strip.gate_enabled ? pre > strip.gate_threshold_db : true;
    const overshoot = strip.comp_enabled ? Math.max(0, pre - strip.comp_threshold_db) : 0;
    const grDb = strip.comp_enabled ? Math.min(12, overshoot - overshoot / strip.comp_ratio) : 0;
    return {
      pre_db: round1(pre),
      post_db: round1(post),
      gate_open: gateOpen,
      gr_db: round1(grDb),
    };
  });

  const buses = state.buses.map((bus, bi) => {
    const activePosts = strips
      .filter((_, i) => !state.strips[i].mute)
      .map((s) => s.post_db);
    const avg = activePosts.length
      ? activePosts.reduce((a, b) => a + b, 0) / activePosts.length
      : -96;
    const base = bus.mute
      ? -96
      : clampMeterDb(avg + bus.fader_db + Math.sin(tick * 0.2 + bi) * 2);
    return {
      id: bus.id,
      peak_l_db: round1(clampMeterDb(base + Math.random() * 2)),
      peak_r_db: round1(clampMeterDb(base + Math.random() * 2)),
      rms_l_db: round1(clampMeterDb(base - 4)),
      rms_r_db: round1(clampMeterDb(base - 4)),
    };
  });

  return { strips, buses };
}

export function createMockMixerController() {
  let state = createInitialMockState();
  const scenes = new Map();
  const meterListeners = new Set();
  let meterTimer = null;
  let tick = 0;

  function snapshot() {
    const cloned = cloneState(state);
    return { ...cloned, scenes: Array.from(scenes.keys()) };
  }

  function requireStrip(index) {
    const strip = state.strips[index];
    if (!strip) throw new Error(`Unknown strip index: ${index}`);
    return strip;
  }

  function requireBus(bus) {
    const found = state.buses.find((b) => b.id === bus);
    if (!found) throw new Error(`Unknown bus: ${bus}`);
    return found;
  }

  function ensureMeterTimer() {
    if (meterTimer || meterListeners.size === 0) return;
    meterTimer = setInterval(() => {
      tick += 1;
      const payload = buildMockMetersPayload(state, tick);
      meterListeners.forEach((cb) => cb(payload));
    }, 50); // 20 Hz
  }

  function stopMeterTimerIfIdle() {
    if (meterTimer && meterListeners.size === 0) {
      clearInterval(meterTimer);
      meterTimer = null;
    }
  }

  return {
    async getState() {
      return snapshot();
    },
    async setStripParam(index, key, value) {
      requireStrip(index);
      state = applyStripChange(state, index, key, value);
      return null;
    },
    async setStripBool(index, key, value) {
      requireStrip(index);
      state = applyStripChange(state, index, key, Boolean(value));
      return null;
    },
    async renameStrip(index, name) {
      requireStrip(index);
      const trimmed = String(name ?? '').trim().slice(0, 24);
      if (!trimmed) throw new Error('Strip name cannot be empty.');
      state = {
        ...state,
        strips: state.strips.map((s, i) => (i === index ? { ...s, name: trimmed } : s)),
      };
      return null;
    },
    async setBusParam(bus, key, value) {
      requireBus(bus);
      state = {
        ...state,
        buses: state.buses.map((b) => (
          b.id === bus ? { ...b, [key]: clampStripValue(key, value) } : b
        )),
      };
      return null;
    },
    async setBusBool(bus, key, value) {
      requireBus(bus);
      state = {
        ...state,
        buses: state.buses.map((b) => (
          b.id === bus ? { ...b, [key]: Boolean(value) } : b
        )),
      };
      return null;
    },
    async listScenes() {
      return Array.from(scenes.keys());
    },
    async saveScene(name) {
      const error = validateSceneName(name);
      if (error) throw new Error(error);
      scenes.set(name, cloneState(state));
      return null;
    },
    async loadScene(name) {
      const found = scenes.get(name);
      if (!found) throw new Error(`Scene not found: ${name}`);
      state = cloneState(found);
      return snapshot();
    },
    async deleteScene(name) {
      scenes.delete(name);
      return null;
    },
    subscribeMeters(cb) {
      meterListeners.add(cb);
      ensureMeterTimer();
      return {
        dispose() {
          meterListeners.delete(cb);
          stopMeterTimerIfIdle();
        },
      };
    },
  };
}

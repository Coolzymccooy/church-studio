import { buildNativeEngineArgs } from './audioRouting.js';

/** Opening the devices normally takes well under a second; a device that never answers
 *  (a sleeping Bluetooth headset is the usual one) must not leave "start" looking stalled. */
export const ENGINE_START_TIMEOUT_MS = 10000;

export const ENGINE_START_TIMEOUT_MESSAGE =
  'The audio devices did not answer. Check Setup: a Bluetooth headset (AirPods) often cannot open for live audio; choose a wired or USB device.';

export async function startNativeEngine(invoke, selectedDevices, { timeoutMs = ENGINE_START_TIMEOUT_MS } = {}) {
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(ENGINE_START_TIMEOUT_MESSAGE)), timeoutMs);
  });
  try {
    return await Promise.race([invoke('start_audio_engine', buildNativeEngineArgs(selectedDevices)), timeout]);
  } finally {
    clearTimeout(timer);
  }
}

export function describeNativeEngineError(err, fallback = 'Could not start Rust audio engine.') {
  if (!err) return fallback;
  if (typeof err === 'string' && err.trim()) return err;
  if (typeof err.message === 'string' && err.message.trim()) return err.message;
  if (typeof err.error === 'string' && err.error.trim()) return err.error;
  if (typeof err.cause === 'string' && err.cause.trim()) return err.cause;
  if (typeof err.toString === 'function') {
    const text = err.toString();
    if (text && text !== '[object Object]') return text;
  }
  return fallback;
}

export async function stopNativeEngine(invoke) {
  return invoke('stop_audio_engine');
}

export async function getNativeEngineStatus(invoke) {
  return invoke('engine_status');
}

export async function captureNativeNoiseProfile(invoke) {
  return invoke('capture_noise_profile');
}

export async function learnNativeMicFingerprint(invoke) {
  return invoke('learn_mic_fingerprint');
}

export function syncNativeParams(invoke, {
  features,
  isBypassed,
  noiseFloorThreshold,
  inputGainValue,
  gateMode,
}) {
  const setParam = (key, value) => invoke('set_param', { key, value });
  const setParamBool = (key, value) => invoke('set_param_bool', { key, value });

  return Promise.allSettled([
    setParamBool('gate_enabled', features.denoise),
    // Hyper gate: 'speech' = speech only, anything else = balanced.
    setParamBool('gate_speech_only', gateMode === 'speech'),
    setParamBool('noise_enabled', features.denoise),
    // RNNoise follows the same features as in the browser engine.
    setParamBool('neural_denoise', Boolean(features.denoise || features.voicePattern)),
    setParamBool('comp_enabled', true),
    setParamBool('deess_enabled', features.dynamicDeEsser !== false),
    setParamBool('dereverb_enabled', features.dereverb),
    setParamBool('bypass', isBypassed),
    setParam('gate_threshold_db', noiseFloorThreshold),
    setParam('gain_db', inputGainValue > 0 ? 20 * Math.log10(inputGainValue) : -60),
  ]);
}

export function subscribeToNativeMeters(listen, onMeters) {
  let disposed = false;
  let unlisten = null;

  const ready = listen('audio-meters', (event) => {
    if (!disposed) {
      onMeters(event.payload);
    }
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
}

export function createNativeEngineController({ invoke, listen }) {
  return {
    start(selectedDevices) {
      return startNativeEngine(invoke, selectedDevices);
    },
    stop() {
      return stopNativeEngine(invoke);
    },
    restart(selectedDevices) {
      return this.stop().catch(() => {}).then(() => this.start(selectedDevices));
    },
    captureNoiseProfile() {
      return captureNativeNoiseProfile(invoke);
    },
    learnMicFingerprint() {
      return learnNativeMicFingerprint(invoke);
    },
    syncParams(params) {
      return syncNativeParams(invoke, params);
    },
    getStatus() {
      return getNativeEngineStatus(invoke);
    },
    subscribeToMeters(onMeters) {
      return subscribeToNativeMeters(listen, onMeters);
    },
  };
}

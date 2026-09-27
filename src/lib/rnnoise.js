// RNNoise loading for the browser audio engine.
//
// The heavy, browser-only bits (fetching/decoding the WASM binary, resolving
// the AudioWorklet module URL) are wrapped in `loadRnnoiseAssets`, which is
// only ever invoked from App.jsx while running in a real browser. The
// caching/single-flight logic itself is pure and is exercised directly by
// rnnoise.test.js using an injected loader stub, so the test suite never
// needs to import `@sapphi-red/web-noise-suppressor` or touch Vite's
// `?url` asset resolution.

/**
 * Wraps an async loader so it only ever runs once. Concurrent callers while
 * the load is in flight share the same promise; a failed load is not
 * cached, so a later call can retry.
 *
 * @param {() => Promise<any>} loadFn
 * @returns {() => Promise<any>}
 */
export function createCachedLoader(loadFn) {
  let inFlight = null;
  let cached = null;
  let hasCached = false;

  return function load() {
    if (hasCached) return Promise.resolve(cached);
    if (inFlight) return inFlight;

    let starting;
    try {
      // Call loadFn synchronously so an in-flight load starts immediately
      // (before the next microtask), matching real-world Promise semantics.
      starting = Promise.resolve(loadFn());
    } catch (err) {
      return Promise.reject(err);
    }

    inFlight = starting
      .then((result) => {
        cached = result;
        hasCached = true;
        inFlight = null;
        return result;
      })
      .catch((err) => {
        inFlight = null;
        throw err;
      });

    return inFlight;
  };
}

let defaultLoader = null;

/**
 * Loads (and caches) everything needed to run RNNoise in an AudioWorklet:
 * the decoded WASM binary, the `RnnoiseWorkletNode` class, and the worklet
 * module's URL (to pass to `audioContext.audioWorklet.addModule`).
 *
 * Safe to call multiple times — the actual load only happens once.
 *
 * @returns {Promise<{ wasmBinary: ArrayBuffer, RnnoiseWorkletNode: any, workletUrl: string }>}
 */
export function loadRnnoiseAssets() {
  if (!defaultLoader) {
    defaultLoader = createCachedLoader(async () => {
      const [{ loadRnnoise, RnnoiseWorkletNode }, wasmUrlMod, simdUrlMod, workletUrlMod] =
        await Promise.all([
          import('@sapphi-red/web-noise-suppressor'),
          import('@sapphi-red/web-noise-suppressor/rnnoise.wasm?url'),
          import('@sapphi-red/web-noise-suppressor/rnnoise_simd.wasm?url'),
          import('@sapphi-red/web-noise-suppressor/rnnoiseWorklet.js?url'),
        ]);

      const wasmBinary = await loadRnnoise({
        url: wasmUrlMod.default,
        simdUrl: simdUrlMod.default,
      });

      return {
        wasmBinary,
        RnnoiseWorkletNode,
        workletUrl: workletUrlMod.default,
      };
    });
  }

  return defaultLoader();
}

/** Test-only: clears the module-level cache so each test starts fresh. */
export function __resetRnnoiseLoaderForTests() {
  defaultLoader = null;
}

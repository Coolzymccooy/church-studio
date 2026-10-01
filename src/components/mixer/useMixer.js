import { useCallback, useEffect, useRef, useState } from 'react';

import { applyStripChange, clampStripValue, createCoalescer } from '../../lib/mixerEngine.js';

const TOAST_MS = 4000;

/**
 * useMixer(controller) — drives a MixerConsole from any object implementing
 * the mixerEngine controller interface (real Tauri controller or the mock).
 *
 * Meters arrive at ~20 Hz and are kept in a ref (`metersRef`), never in React
 * state, so the console doesn't re-render on every tick — Meter.jsx reads the
 * ref directly from a requestAnimationFrame loop.
 *
 * Every mutation is optimistic: local state updates immediately, the
 * controller call fires in the background, and a failure reverts the local
 * state and raises a short-lived toast message.
 */
export function useMixer(controller) {
  const [state, setState] = useState(null);
  const [loading, setLoading] = useState(true);
  const [toast, setToast] = useState(null);
  const metersRef = useRef({ strips: [], buses: [] });
  const toastTimerRef = useRef(null);
  // Continuous controls (faders, knobs) fire on every input event while
  // dragging; send at most one engine command per control per frame.
  const [coalesce] = useState(() => createCoalescer());
  const stripCountRef = useRef(0);

  useEffect(() => {
    stripCountRef.current = state?.strips?.length ?? 0;
  }, [state]);

  const showToast = useCallback((message) => {
    setToast(message);
    if (toastTimerRef.current) clearTimeout(toastTimerRef.current);
    toastTimerRef.current = setTimeout(() => setToast(null), TOAST_MS);
  }, []);

  const refresh = useCallback(async () => {
    const next = await controller.getState();
    setState(next);
    return next;
  }, [controller]);

  useEffect(() => {
    let cancelled = false;
    controller.getState()
      .then((next) => {
        if (!cancelled) setState(next);
      })
      .catch((err) => {
        if (!cancelled) showToast(describeMixerError(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    // The engine can start, stop or change device while this view is open,
    // which changes how many strips exist. Meter frames carry one entry per
    // running strip, so a count that differs from the state means the state
    // is stale: fetch it again (once at a time).
    let refetching = false;
    const sub = controller.subscribeMeters((payload) => {
      metersRef.current = payload;
      const running = payload?.strips?.length ?? 0;
      if (running > 0 && running !== stripCountRef.current && !refetching && !cancelled) {
        refetching = true;
        controller.getState()
          .then((next) => { if (!cancelled) setState(next); })
          .catch(() => {})
          .finally(() => { refetching = false; });
      }
    });

    // The backend loaded a scene on its own (Tiwaton Link or the OBS scene
    // link): refetch so the console shows it.
    const refetch = () => {
      if (cancelled) return;
      controller.getState()
        .then((next) => { if (!cancelled) setState(next); })
        .catch(() => {});
    };
    const stateSub = controller.subscribeStateChanges?.(refetch);
    const sceneSub = controller.subscribeSceneLoaded?.(refetch);

    return () => {
      cancelled = true;
      sub.dispose();
      stateSub?.dispose();
      sceneSub?.dispose();
      if (toastTimerRef.current) clearTimeout(toastTimerRef.current);
    };
  }, [controller, showToast]);

  // withOptimism applies an optimistic change immediately and, on failure,
  // reverts only the field this specific call touched — and reverts it
  // against whatever the *latest* state is at that point, not a stale
  // snapshot taken before the call started. Rapid slider drags fire many
  // overlapping invokes; capturing one full-state snapshot up front and
  // restoring it wholesale on failure would silently discard every other
  // change (from this call or others) that landed in between. buildRevert
  // receives the pre-change state so it can capture the old value, and
  // returns a function that reapplies just that value onto the latest state.
  const withOptimism = useCallback((applyChange, buildRevert, call) => {
    let revert = null;
    setState((prev) => {
      if (!prev) return prev;
      revert = buildRevert(prev);
      return applyChange(prev);
    });
    call().catch((err) => {
      showToast(describeMixerError(err));
      if (revert) setState((prev) => (prev ? revert(prev) : prev));
      // With coalescing, the value before this change may never have reached
      // the engine; re-read the engine so the UI shows what it really has.
      controller.getState().then(setState).catch(() => {});
    });
  }, [controller, showToast]);

  const setStripParam = useCallback((index, key, value) => {
    withOptimism(
      (prev) => applyStripChange(prev, index, key, value),
      (prev) => {
        const previousValue = prev.strips[index]?.[key];
        return (latest) => applyStripChange(latest, index, key, previousValue);
      },
      () => coalesce(`strip:${index}:${key}`, () => controller.setStripParam(index, key, value)),
    );
  }, [controller, coalesce, withOptimism]);

  const setStripBool = useCallback((index, key, value) => {
    withOptimism(
      (prev) => applyStripChange(prev, index, key, Boolean(value)),
      (prev) => {
        const previousValue = prev.strips[index]?.[key];
        return (latest) => applyStripChange(latest, index, key, previousValue);
      },
      () => controller.setStripBool(index, key, value),
    );
  }, [controller, withOptimism]);

  const renameStrip = useCallback((index, name) => {
    const trimmed = String(name ?? '').trim().slice(0, 24);
    withOptimism(
      (prev) => ({
        ...prev,
        strips: prev.strips.map((s, i) => (i === index ? { ...s, name: trimmed } : s)),
      }),
      (prev) => {
        const previousName = prev.strips[index]?.name;
        return (latest) => ({
          ...latest,
          strips: latest.strips.map((s, i) => (i === index ? { ...s, name: previousName } : s)),
        });
      },
      () => controller.renameStrip(index, trimmed),
    );
  }, [controller, withOptimism]);

  const setBusParam = useCallback((bus, key, value) => {
    withOptimism(
      (prev) => ({
        ...prev,
        buses: prev.buses.map((b) => (b.id === bus ? { ...b, [key]: clampStripValue(key, value) } : b)),
      }),
      (prev) => {
        const previousValue = prev.buses.find((b) => b.id === bus)?.[key];
        return (latest) => ({
          ...latest,
          buses: latest.buses.map((b) => (b.id === bus ? { ...b, [key]: previousValue } : b)),
        });
      },
      () => coalesce(`bus:${bus}:${key}`, () => controller.setBusParam(bus, key, value)),
    );
  }, [controller, coalesce, withOptimism]);

  const setBusBool = useCallback((bus, key, value) => {
    withOptimism(
      (prev) => ({
        ...prev,
        buses: prev.buses.map((b) => (b.id === bus ? { ...b, [key]: Boolean(value) } : b)),
      }),
      (prev) => {
        const previousValue = prev.buses.find((b) => b.id === bus)?.[key];
        return (latest) => ({
          ...latest,
          buses: latest.buses.map((b) => (b.id === bus ? { ...b, [key]: previousValue } : b)),
        });
      },
      () => controller.setBusBool(bus, key, value),
    );
  }, [controller, withOptimism]);

  const saveScene = useCallback(async (name) => {
    try {
      await controller.saveScene(name);
      await refresh();
      return true;
    } catch (err) {
      showToast(describeMixerError(err));
      return false;
    }
  }, [controller, refresh, showToast]);

  const loadScene = useCallback(async (name) => {
    try {
      const next = await controller.loadScene(name);
      setState(next);
      return true;
    } catch (err) {
      showToast(describeMixerError(err));
      return false;
    }
  }, [controller, showToast]);

  const deleteScene = useCallback(async (name) => {
    try {
      await controller.deleteScene(name);
      await refresh();
      return true;
    } catch (err) {
      showToast(describeMixerError(err));
      return false;
    }
  }, [controller, refresh, showToast]);

  return {
    state,
    loading,
    toast,
    dismissToast: () => setToast(null),
    metersRef,
    setStripParam,
    setStripBool,
    renameStrip,
    setBusParam,
    setBusBool,
    saveScene,
    loadScene,
    deleteScene,
  };
}

function describeMixerError(err) {
  if (!err) return 'Mixer command failed.';
  if (typeof err === 'string') return err;
  if (typeof err.message === 'string' && err.message) return err.message;
  return 'Mixer command failed.';
}

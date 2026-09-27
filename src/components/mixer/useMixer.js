import { useCallback, useEffect, useRef, useState } from 'react';

import { applyStripChange } from '../../lib/mixerEngine.js';

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

    const sub = controller.subscribeMeters((payload) => {
      metersRef.current = payload;
    });

    return () => {
      cancelled = true;
      sub.dispose();
      if (toastTimerRef.current) clearTimeout(toastTimerRef.current);
    };
  }, [controller, showToast]);

  const withOptimism = useCallback((optimisticUpdate, call) => {
    let previous;
    setState((prev) => {
      previous = prev;
      return prev ? optimisticUpdate(prev) : prev;
    });
    call().catch((err) => {
      showToast(describeMixerError(err));
      setState(previous ?? null);
    });
  }, [showToast]);

  const setStripParam = useCallback((index, key, value) => {
    withOptimism(
      (prev) => applyStripChange(prev, index, key, value),
      () => controller.setStripParam(index, key, value),
    );
  }, [controller, withOptimism]);

  const setStripBool = useCallback((index, key, value) => {
    withOptimism(
      (prev) => applyStripChange(prev, index, key, Boolean(value)),
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
      () => controller.renameStrip(index, trimmed),
    );
  }, [controller, withOptimism]);

  const setBusParam = useCallback((bus, key, value) => {
    withOptimism(
      (prev) => ({
        ...prev,
        buses: prev.buses.map((b) => (b.id === bus ? { ...b, [key]: value } : b)),
      }),
      () => controller.setBusParam(bus, key, value),
    );
  }, [controller, withOptimism]);

  const setBusBool = useCallback((bus, key, value) => {
    withOptimism(
      (prev) => ({
        ...prev,
        buses: prev.buses.map((b) => (b.id === bus ? { ...b, [key]: Boolean(value) } : b)),
      }),
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

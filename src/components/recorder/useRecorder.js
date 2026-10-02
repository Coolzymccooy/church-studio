import { useCallback, useEffect, useState } from 'react';

import {
  EMPTY_RECORDER_STATUS,
  describeRecorderError,
  followRecorderStatus,
  normalizeRecorderConfig,
  normalizeRecorderStatus,
} from '../../lib/recorder.js';

const IDLE_POLL_MS = 2000;

/**
 * useRecorderStatus(controller) — live recorder status (`recorder-status`
 * events plus a snapshot). Pass null to stay idle (browser build).
 */
export function useRecorderStatus(controller) {
  const [status, setStatus] = useState(EMPTY_RECORDER_STATUS);

  useEffect(() => {
    if (!controller) return undefined;
    return followRecorderStatus(controller, setStatus);
  }, [controller]);

  return [status, setStatus];
}

/**
 * useRecorder(controller) — status, config and the actions behind the
 * Record section. Every action reports failures in `error` instead of
 * throwing; `busy` is true while a command runs.
 */
export function useRecorder(controller) {
  const [status, setStatus] = useRecorderStatus(controller);
  const [config, setConfig] = useState(null);
  const [error, setError] = useState(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!controller) return undefined;
    let cancelled = false;
    controller.getConfig()
      .then((raw) => { if (!cancelled) setConfig(normalizeRecorderConfig(raw)); })
      .catch((err) => { if (!cancelled) setError(describeRecorderError(err)); });
    return () => { cancelled = true; };
  }, [controller]);

  // Events only flow while recording; poll when idle so the section notices
  // the engine starting or stopping.
  const recording = status.recording;
  useEffect(() => {
    if (!controller || recording) return undefined;
    let cancelled = false;
    const timer = setInterval(() => {
      controller.getStatus()
        .then((raw) => { if (!cancelled) setStatus(normalizeRecorderStatus(raw)); })
        .catch(() => {});
    }, IDLE_POLL_MS);
    return () => { cancelled = true; clearInterval(timer); };
  }, [controller, recording, setStatus]);

  const run = useCallback(async (action) => {
    setBusy(true);
    setError(null);
    try {
      return await action();
    } catch (err) {
      setError(describeRecorderError(err));
      return null;
    } finally {
      setBusy(false);
    }
  }, []);

  const start = useCallback((title) => run(async () => {
    setStatus(normalizeRecorderStatus(await controller.start({ title })));
  }), [controller, run, setStatus]);

  const stop = useCallback(() => run(async () => {
    setStatus(normalizeRecorderStatus(await controller.stop()));
  }), [controller, run, setStatus]);

  const addMarker = useCallback((label) => run(() => controller.addMarker(label)), [controller, run]);

  const saveConfig = useCallback((next) => run(async () => {
    setConfig(normalizeRecorderConfig(await controller.setConfig(next)));
  }), [controller, run]);

  const openFolder = useCallback(() => run(() => controller.openFolder()), [controller, run]);

  return {
    status,
    config,
    error,
    busy,
    start,
    stop,
    addMarker,
    saveConfig,
    openFolder,
    dismissError: () => setError(null),
  };
}

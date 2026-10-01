import { useCallback, useEffect, useState } from 'react';

import {
  appendActivity,
  createLinkController,
  describeLinkError,
} from '../../lib/linkControl.js';

/** Real Tauri controller; invoke/listen are loaded lazily (desktop only). */
export function createTauriLinkController() {
  const invoke = (command, payload) => (
    import('@tauri-apps/api/core').then(({ invoke: tauriInvoke }) => tauriInvoke(command, payload))
  );
  const listen = (event, cb) => (
    import('@tauri-apps/api/event').then(({ listen: tauriListen }) => tauriListen(event, cb))
  );
  return createLinkController({ invoke, listen });
}

/**
 * useLinkPanel(controller, open) — state and actions for the Link panel.
 * Loads config, activity and scenes when opened, then follows the
 * `link-activity` / `link-status` events while open.
 */
export function useLinkPanel(controller, open) {
  const [config, setConfig] = useState(null);
  const [entries, setEntries] = useState([]);
  const [scenes, setScenes] = useState([]);
  const [lastEventAt, setLastEventAt] = useState(null);
  const [error, setError] = useState(null);
  const [now, setNow] = useState(() => Date.now());

  const fail = useCallback((err) => setError(describeLinkError(err)), []);

  const reload = useCallback(() => {
    if (!controller) return Promise.resolve();
    return Promise.all([controller.getConfig(), controller.getActivity(), controller.listScenes()])
      .then(([cfg, activity, sceneList]) => {
        setConfig(cfg);
        setEntries(activity?.entries ?? []);
        setLastEventAt(activity?.lastEventAt ?? cfg?.lastEventAt ?? null);
        setScenes(Array.isArray(sceneList) ? sceneList : []);
        setError(null);
      })
      .catch(fail);
  }, [controller, fail]);

  useEffect(() => {
    if (!open || !controller) return undefined;
    let cancelled = false;
    reload();
    const activitySub = controller.subscribeActivity((entry) => {
      if (!cancelled && entry) setEntries((prev) => appendActivity(prev, entry));
    });
    const statusSub = controller.subscribeStatus((status) => {
      if (cancelled || !status) return;
      setLastEventAt(status.lastEventAt ?? null);
      setConfig((prev) => (prev ? { ...prev, paused: Boolean(status.paused) } : prev));
    });
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => {
      cancelled = true;
      activitySub.dispose();
      statusSub.dispose();
      clearInterval(timer);
    };
  }, [open, controller, reload]);

  const run = useCallback(async (promise, apply) => {
    try {
      const result = await promise;
      apply?.(result);
      setError(null);
      return true;
    } catch (err) {
      fail(err);
      return false;
    }
  }, [fail]);

  const setEnabled = (enabled) => run(controller.setEnabled(enabled), setConfig);
  const setPaused = (paused) => run(controller.setAutomationPaused(paused), setConfig);
  const regenerateToken = () => run(controller.regenerateToken(), setConfig);
  const saveRules = (rules) => run(
    controller.setRules(rules),
    (saved) => setConfig((prev) => (prev ? { ...prev, rules: saved } : prev)),
  );
  const undo = (entryId) => run(controller.undo(entryId));

  return {
    config,
    entries,
    scenes,
    lastEventAt,
    error,
    now,
    reload,
    setEnabled,
    setPaused,
    regenerateToken,
    saveRules,
    undo,
  };
}

import React, { useCallback, useEffect, useState } from 'react';
import { MonitorPlay, X } from 'lucide-react';

import { isTauri } from '../../lib/platform';
import {
  OBS_OUTPUT_ACTIONS,
  describeObsError,
  getDesktopObsController,
  normalizeSceneLink,
} from '../../lib/obsControl.js';
import ObsConfirmDialog from './ObsConfirmDialog.jsx';
import ObsConnectionSection from './ObsConnectionSection.jsx';
import ObsOutputSection from './ObsOutputSection.jsx';
import ObsSceneLinkSection from './ObsSceneLinkSection.jsx';
import { useObsStatus } from './useObsStatus.js';

function Shell({ onClose, children }) {
  useEffect(() => {
    const onKey = (e) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  return (
    <div className="fixed inset-0 bg-black/70 z-50 flex items-center justify-center backdrop-blur-sm p-4">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="obs-panel-title"
        className="relative w-full max-w-xl max-h-[90vh] overflow-y-auto rounded-2xl border border-slate-700 bg-slate-900 p-6 space-y-5 shadow-2xl"
      >
        <div className="flex items-center justify-between">
          <h2 id="obs-panel-title" className="flex items-center gap-2 text-lg font-bold text-white">
            <MonitorPlay size={20} className="text-[var(--accent)]" />
            OBS Studio
          </h2>
          <button type="button" onClick={onClose} className="text-slate-400 hover:text-white" aria-label="Close">
            <X size={22} />
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}

function DesktopOnly({ onClose }) {
  return (
    <Shell onClose={onClose}>
      <p className="text-sm text-slate-300 leading-relaxed">
        OBS control needs the TIWATON AI Studio desktop app. It connects to OBS on this computer or your
        network through OBS&apos;s WebSocket server, which a browser tab can&apos;t do.
      </p>
    </Shell>
  );
}

function DesktopPanel({ onClose, mixerController }) {
  const controller = getDesktopObsController();
  const status = useObsStatus(controller);
  const [config, setConfig] = useState(null);
  const [studioScenes, setStudioScenes] = useState([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(null);
  const [confirmKind, setConfirmKind] = useState(null);

  useEffect(() => {
    let cancelled = false;
    controller.getConfig()
      .then((next) => { if (!cancelled) setConfig(next); })
      .catch((err) => { if (!cancelled) setError(describeObsError(err)); });
    mixerController?.listScenes?.()
      .then((names) => { if (!cancelled) setStudioScenes(names || []); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [controller, mixerController]);

  const run = useCallback(async (fn) => {
    setBusy(true);
    setError(null);
    try {
      return await fn();
    } catch (err) {
      setError(describeObsError(err));
      return null;
    } finally {
      setBusy(false);
    }
  }, []);

  const saveConfig = (patch) => run(async () => {
    const next = await controller.setConfig({
      host: config?.host,
      port: config?.port,
      password: null,
      enabled: config?.enabled,
      ...patch,
    });
    if (next) setConfig(next);
  });

  const link = normalizeSceneLink(config?.sceneLink);
  const onLinkChange = (nextLink) => {
    setConfig((prev) => (prev ? { ...prev, sceneLink: nextLink } : prev));
    saveConfig({ sceneLink: nextLink });
  };

  const confirmAction = () => {
    const kind = confirmKind;
    setConfirmKind(null);
    if (kind) run(() => controller[kind](true));
  };

  // Escape cancels an open confirm first, then closes the panel.
  const onEscapeOrClose = confirmKind ? () => setConfirmKind(null) : onClose;

  return (
    <Shell onClose={onEscapeOrClose}>
      <ObsConnectionSection
        key={config ? `${config.host}:${config.port}` : 'loading'}
        config={config}
        status={status}
        busy={busy || !config}
        onConnect={(fields) => saveConfig({ ...fields, enabled: true })}
        onDisconnect={() => saveConfig({ enabled: false })}
        onClearPassword={() => saveConfig({ password: '' })}
      />
      <div className="border-t border-slate-800" />
      <ObsOutputSection
        status={status}
        busy={busy}
        onSetScene={(name) => run(() => controller.setScene(name))}
        onRequestAction={setConfirmKind}
      />
      <div className="border-t border-slate-800" />
      <ObsSceneLinkSection
        link={link}
        studioScenes={studioScenes}
        obsScenes={status.scenes || []}
        busy={busy || !config}
        onChange={onLinkChange}
      />
      {error && <p className="text-[11px] text-red-400" role="alert">{error}</p>}
      <ObsConfirmDialog
        request={confirmKind ? OBS_OUTPUT_ACTIONS[confirmKind] : null}
        onConfirm={confirmAction}
        onCancel={() => setConfirmKind(null)}
      />
    </Shell>
  );
}

/** Tools → OBS Studio… modal. In the browser it explains the desktop app is needed. */
export default function ObsPanel({ open, onClose, mixerController }) {
  if (!open) return null;
  if (!isTauri) return <DesktopOnly onClose={onClose} />;
  return <DesktopPanel onClose={onClose} mixerController={mixerController} />;
}

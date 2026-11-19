import React, { useCallback, useEffect, useState } from 'react';

import { isTauri } from '../../lib/platform';
import {
  OBS_OUTPUT_ACTIONS,
  describeObsError,
  getDesktopObsController,
  normalizeSceneLink,
} from '../../lib/obsControl.js';
import InfoTip from '../faceplate/InfoTip.jsx';
import Key from '../faceplate/Key.jsx';
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
    <>
      <div className="fp-scrim" aria-hidden="true" />
      <div className="fp-dialog-wrap">
        <div
          className="fp-dialog"
          style={{ width: 'min(576px, 100%)' }}
          role="dialog"
          aria-modal="true"
          aria-labelledby="obs-panel-title"
        >
          <div className="fp-dialog-hd">
            <span id="obs-panel-title">OBS Studio</span>
            <Key onClick={onClose} aria-label="close OBS Studio">close</Key>
          </div>
          <div className="fp-dialog-bd">{children}</div>
        </div>
      </div>
    </>
  );
}

function DesktopOnly({ onClose }) {
  return (
    <Shell onClose={onClose}>
      <div className="fp-row">
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6, overflow: 'visible' }}>
          Desktop app only
          <InfoTip label="OBS control">
            OBS control needs the TIWATON AI Studio desktop app. It connects to OBS on this computer or
            your network through OBS&apos;s WebSocket server, which a browser tab can&apos;t do.
          </InfoTip>
        </span>
      </div>
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
      <ObsOutputSection
        status={status}
        busy={busy}
        onSetScene={(name) => run(() => controller.setScene(name))}
        onRequestAction={setConfirmKind}
      />
      <ObsSceneLinkSection
        link={link}
        studioScenes={studioScenes}
        obsScenes={status.scenes || []}
        busy={busy || !config}
        onChange={onLinkChange}
      />
      {error && <p className="fp-sub fp-fault" style={{ margin: 0 }} role="alert">{error}</p>}
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

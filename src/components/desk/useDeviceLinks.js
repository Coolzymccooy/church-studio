import { useCallback, useEffect, useRef, useState } from 'react';

import { isTauri } from '../../lib/platform';
import { normalizeTauriDevices } from '../../lib/audioRouting.js';
import { trackUnderruns } from '../../lib/usbDevices.js';

const EMPTY = { links: [], slipping: false };

/** Listen to one Tauri event while `active`; unlistens on cleanup, even if it lands late. */
function useTauriEvent(name, active, onEvent) {
  useEffect(() => {
    if (!isTauri || !active) return undefined;
    let disposed = false;
    let unlisten = null;
    import('@tauri-apps/api/event')
      .then(({ listen }) => listen(name, (event) => { if (!disposed) onEvent(event.payload); }))
      .then((fn) => { if (disposed) fn(); else unlisten = fn; })
      .catch(() => {});
    return () => {
      disposed = true;
      if (unlisten) unlisten();
    };
  }, [name, active, onEvent]);
}

/**
 * Each engine stream's device and link state (engine_status.devices) plus whether a clock slipped
 * in the last 10 s. `update` takes a status list (from the 1 s poll); `audio-device-health`
 * updates it at once. Empty while the engine is off.
 */
export function useDeviceLinks(isLive) {
  const [state, setState] = useState(EMPTY);
  const historyRef = useRef([]);

  const update = useCallback((devices) => {
    const links = Array.isArray(devices) ? devices : [];
    const { history, slipping } = trackUnderruns(historyRef.current, links, Date.now());
    historyRef.current = history;
    setState({ links, slipping });
  }, []);

  useTauriEvent('audio-device-health', isLive, update);
  return { ...(isLive ? state : EMPTY), update };
}

/** Hot-plug: refresh the device lists when the engine's watcher sees a device come or go. */
export function useDeviceHotplug(onDevices) {
  const onEvent = useCallback((payload) => {
    onDevices({
      inputs: normalizeTauriDevices(payload?.inputs ?? []),
      outputs: normalizeTauriDevices(payload?.outputs ?? []),
    });
  }, [onDevices]);
  useTauriEvent('audio-devices-changed', true, onEvent);
}

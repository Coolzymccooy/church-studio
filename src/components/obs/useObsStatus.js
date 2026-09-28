import { useEffect, useState } from 'react';

import { EMPTY_OBS_STATUS } from '../../lib/obsControl.js';

/**
 * useObsStatus(controller) — live OBS status: the current snapshot from
 * `obs_status`, then every `obs-status` event. Pass null to stay idle
 * (browser build).
 */
export function useObsStatus(controller) {
  const [status, setStatus] = useState(EMPTY_OBS_STATUS);

  useEffect(() => {
    if (!controller) return undefined;
    let cancelled = false;
    const sub = controller.subscribeStatus((next) => {
      if (!cancelled && next) setStatus(next);
    });
    controller.getStatus()
      .then((next) => { if (!cancelled && next) setStatus(next); })
      .catch(() => {});
    return () => {
      cancelled = true;
      sub.dispose();
    };
  }, [controller]);

  return status;
}

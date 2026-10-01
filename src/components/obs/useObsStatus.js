import { useEffect, useState } from 'react';

import { EMPTY_OBS_STATUS, followObsStatus } from '../../lib/obsControl.js';

/**
 * useObsStatus(controller) — live OBS status via `followObsStatus`: every
 * `obs-status` event, plus the `obs_status` snapshot taken once listening.
 * Pass null to stay idle (browser build).
 */
export function useObsStatus(controller) {
  const [status, setStatus] = useState(EMPTY_OBS_STATUS);

  useEffect(() => {
    if (!controller) return undefined;
    return followObsStatus(controller, setStatus);
  }, [controller]);

  return status;
}

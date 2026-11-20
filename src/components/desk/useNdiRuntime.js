import { useEffect, useState } from 'react';

import { isTauri } from '../../lib/platform';
import { createNdiClient } from '../../lib/ndiOutput';

const RETRY_MS = 500;

/**
 * Whether the NDI runtime is available, for rail LEDs and alerts: { checked, available }.
 * The runtime loads in the background, so it asks again until the answer is final. Browser: an
 * unchecked state, so no NDI alert shows there.
 */
export default function useNdiRuntime() {
  const [runtime, setRuntime] = useState({ checked: false, available: false });

  useEffect(() => {
    if (!isTauri) return undefined;
    let cancelled = false;
    let retry = null;
    import('@tauri-apps/api/core').then(({ invoke }) => {
      const client = createNdiClient(invoke);
      const ask = () => {
        client.getStatus()
          .then((status) => {
            if (cancelled) return;
            if (status?.loading) retry = setTimeout(ask, RETRY_MS);
            else setRuntime({ checked: true, available: Boolean(status?.available) });
          })
          .catch(() => { if (!cancelled) setRuntime({ checked: true, available: false }); });
      };
      ask();
    });
    return () => { cancelled = true; clearTimeout(retry); };
  }, []);

  return runtime;
}

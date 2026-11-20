import { useEffect, useState } from 'react';
import { isTauri } from '../../lib/platform.js';

const empty = { selected: 0, playing: false };

export function useSoundcheckSignal(engineOn) {
  const [signal, setSignal] = useState(empty);
  useEffect(() => {
    if (!engineOn || !isTauri) return undefined;
    let active = true;
    let pending = false;
    const refresh = async () => {
      if (pending) return;
      pending = true;
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const next = await invoke('soundcheck_signal');
        if (active && next) setSignal((prev) => (
          prev.selected === next.selected && prev.playing === next.playing ? prev : next
        ));
      } catch {
        // Engine start/stop can briefly hold the lock.
      } finally {
        pending = false;
      }
    };
    void refresh();
    const timer = setInterval(refresh, 150);
    return () => { active = false; clearInterval(timer); };
  }, [engineOn]);
  return engineOn ? signal : empty;
}

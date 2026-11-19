import { useEffect, useRef } from 'react';

const EMPTY = { strips: [], buses: [] };

/**
 * Keeps the latest mixer meter frame in a ref while `enabled`, without re-rendering. Readers call
 * the returned getters from animation frames.
 */
export default function useMeterFeed(controller, enabled) {
  const ref = useRef(EMPTY);

  useEffect(() => {
    if (!enabled || !controller?.subscribeMeters) return undefined;
    const sub = controller.subscribeMeters((payload) => { ref.current = payload || EMPTY; });
    return () => {
      ref.current = EMPTY;
      sub?.dispose?.();
    };
  }, [controller, enabled]);

  return ref;
}

/** The loudest input strip before its fader, in dBFS (-96 when silent or stopped). */
export function inputPeakDb(frame) {
  const strips = frame?.strips || [];
  return strips.reduce((max, s) => Math.max(max, s?.pre_db ?? -96), -96);
}

/** The louder side of one bus, in dBFS. */
export function busPeakDb(frame, id) {
  const bus = (frame?.buses || []).find((b) => b.id === id);
  if (!bus) return -96;
  return Math.max(bus.peak_l_db ?? -96, bus.peak_r_db ?? -96);
}

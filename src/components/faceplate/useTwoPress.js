import { useCallback, useEffect, useState } from 'react';
import { ARM_TIMEOUT_MS, IDLE, pressTwoPress } from '../../lib/faceplateControls.js';

/** Two-press state for a destructive key: press() arms, a second press() inside the window confirms. */
export default function useTwoPress(onConfirm, timeoutMs = ARM_TIMEOUT_MS) {
  const [state, setState] = useState(IDLE);

  useEffect(() => {
    if (state.armedAt === null) return undefined;
    const timer = setTimeout(() => setState(IDLE), timeoutMs);
    return () => clearTimeout(timer);
  }, [state, timeoutMs]);

  const press = useCallback(() => {
    const result = pressTwoPress(state, Date.now(), timeoutMs);
    setState(result.state);
    if (result.fire) onConfirm();
  }, [state, onConfirm, timeoutMs]);

  const cancel = useCallback(() => setState(IDLE), []);

  return { armed: state.armedAt !== null, press, cancel };
}

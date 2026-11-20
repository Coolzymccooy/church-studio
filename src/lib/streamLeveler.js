/**
 * Stream loudness leveler: the "Stream safe" switch and the loudness target drive the native
 * Stream bus leveler (`leveler_enabled`, `leveler_target_lufs` in
 * docs/specs/2026-09-27-mixer-live-contract.md).
 */
export const LEVELER_BUS = 'stream';
export const LEVELER_TARGET_MIN = -30;
export const LEVELER_TARGET_MAX = -10;
export const LEVELER_TARGET_DEFAULT = -14;

/** Clamp a target to the leveler range; anything not a finite number gives the default. */
export function levelerTarget(target) {
  if (typeof target !== 'number' || !Number.isFinite(target)) return LEVELER_TARGET_DEFAULT;
  return Math.min(LEVELER_TARGET_MAX, Math.max(LEVELER_TARGET_MIN, target));
}

/**
 * Send the switch and target to the mixer controller. Target first, so a switch turned on
 * never levels toward a stale target. Resolves to null, or the error message on failure.
 */
export async function syncStreamLeveler(controller, { on, target }) {
  try {
    await controller.setBusParam(LEVELER_BUS, 'leveler_target_lufs', levelerTarget(target));
    await controller.setBusBool(LEVELER_BUS, 'leveler_enabled', Boolean(on));
    return null;
  } catch (err) {
    return err instanceof Error ? err.message : String(err);
  }
}

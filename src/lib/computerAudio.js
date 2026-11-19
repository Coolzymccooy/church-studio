/**
 * Computer audio input (desktop engine): what this computer plays becomes
 * a stereo pair of mixer strips at the next engine start.
 *
 * Tauri commands:
 *   computer_audio_get()          → { enabled, permission, running, restart_required }
 *   computer_audio_set({enabled}) → { enabled, permission, running, restart_required, message, save_error }
 *
 * `permission` is "granted", "denied" or "unknown" (macOS System Audio
 * Recording; always "granted" on Windows).
 */

const PERMISSIONS = new Set(['granted', 'denied', 'unknown']);

/** A command result → the settings the UI keeps. */
export function normalizeComputerAudio(result) {
  const permission = PERMISSIONS.has(result?.permission) ? result.permission : 'unknown';
  return {
    enabled: Boolean(result?.enabled),
    permission,
    running: Boolean(result?.running),
    restartRequired: Boolean(result?.restart_required),
    saveError: typeof result?.save_error === 'string' ? result.save_error : null,
  };
}

/**
 * The lamp for the current settings: `tone` (g, a, r or off) and the state
 * in a word or three. `error` is a failed command's message.
 */
export function computerAudioLamp(settings, error = null) {
  if (error) return { tone: 'r', word: 'Not changed' };
  if (settings?.saveError) return { tone: 'a', word: 'Not saved' };
  if (settings?.enabled && settings.permission === 'denied') {
    return { tone: 'r', word: 'Needs permission' };
  }
  if (settings?.restartRequired) return { tone: 'a', word: 'Restart to apply' };
  return settings?.enabled ? { tone: 'g', word: 'On' } : { tone: 'off', word: 'Off' };
}

/** Thin wrapper around the Tauri commands. */
export function createComputerAudioClient(invoke) {
  return {
    get: async () => normalizeComputerAudio(await invoke('computer_audio_get')),
    set: async (enabled) => normalizeComputerAudio(
      await invoke('computer_audio_set', { enabled: Boolean(enabled) }),
    ),
  };
}

/**
 * platform.js — Unified abstraction over Tauri desktop APIs and plain browser.
 *
 * Usage:
 *   import { isTauri, onMenuCommand, openExternal } from './lib/platform';
 *
 * All functions degrade gracefully when running in a plain browser tab.
 */

// ── Environment detection ──────────────────────────────────────────────────
export const isTauri = Boolean(window.__TAURI__);

// Legacy Electron compatibility (unused after migration but kept for safety)
export const isElectron = Boolean(window.electronAPI?.isElectron);

export const isDesktopApp = isTauri || isElectron;

// ── Native platform string ('windows' | 'macos' | 'linux' | 'web') ────────
let _platform = 'web';
if (isTauri) {
  // Resolved asynchronously below; synchronous fallback from user agent
  _platform = navigator.userAgent.toLowerCase().includes('windows')
    ? 'windows'
    : navigator.userAgent.toLowerCase().includes('mac')
    ? 'macos'
    : 'linux';

  // Upgrade to accurate Tauri OS value asynchronously
  import('@tauri-apps/plugin-os').then(({ platform }) => {
    platform().then(p => { _platform = p; }).catch(() => {});
  }).catch(() => {});
}
export const getPlatform = () => _platform;

// ── Menu command listener ─────────────────────────────────────────────────
/**
 * Subscribe to native menu commands.
 * @param {(event: string, ...args: string[]) => void} callback
 * @returns {() => void} unsubscribe function
 */
export function onMenuCommand(callback) {
  if (isTauri) {
    let unlisten = null;
    import('@tauri-apps/api/event').then(({ listen }) => {
      listen('menu-event', (e) => {
        const { event, args = [] } = e.payload;
        callback(event, ...args);
      }).then(fn => { unlisten = fn; });
    });
    return () => { if (unlisten) unlisten(); };
  }

  if (isElectron && window.electronAPI?.onMenuCommand) {
    return window.electronAPI.onMenuCommand(callback);
  }

  // Browser — no native menu, nothing to subscribe to
  return () => {};
}

// ── Open URL in system browser ────────────────────────────────────────────
export async function openExternal(url) {
  if (isTauri) {
    const { open } = await import('@tauri-apps/plugin-shell');
    return open(url);
  }
  window.open(url, '_blank', 'noopener,noreferrer');
}

// ── Save file dialog ──────────────────────────────────────────────────────
/**
 * Show OS save dialog. Returns chosen file path string, or null if cancelled.
 * In browser, returns null (browser uses its own download dialog).
 */
export async function showSaveDialog({ defaultName = 'file', filters = [] } = {}) {
  if (isTauri) {
    const { save } = await import('@tauri-apps/plugin-dialog');
    return save({ defaultPath: defaultName, filters });
  }
  if (isElectron && window.electronAPI?.showSaveDialog) {
    const result = await window.electronAPI.showSaveDialog({ defaultName, filters });
    return result?.canceled ? null : result?.filePath ?? null;
  }
  return null;
}

// ── Window drag region helper ─────────────────────────────────────────────
/**
 * Returns the data attribute to apply to a drag-able titlebar element.
 * Only meaningful in Tauri (decorations: false). In browser, returns {}.
 */
export const dragRegionProps = isTauri ? { 'data-tauri-drag-region': '' } : {};

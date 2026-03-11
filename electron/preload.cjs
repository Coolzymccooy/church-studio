const { contextBridge, ipcRenderer } = require('electron');

// Expose safe Electron APIs to renderer
contextBridge.exposeInMainWorld('electronAPI', {
  // File save dialog
  showSaveDialog: (opts) => ipcRenderer.invoke('dialog:save', opts),

  // Listen for menu commands sent from Electron main menu
  onMenuCommand: (callback) => {
    const events = [
      'menu:show-settings',
      'menu:toggle-live',
      'menu:hard-reset',
      'menu:calibrate',
      'menu:learn-mic',
      'menu:capture-noise',
      'menu:set-tab',
      'menu:toggle-bypass',
      'menu:export-wav',
      'menu:export-webm',
      'menu:export-mp4',
      'menu:snapshot',
      'menu:save-snapshot',
    ];
    const handlers = events.map(event => {
      const handler = (_, ...args) => callback(event, ...args);
      ipcRenderer.on(event, handler);
      return { event, handler };
    });
    // Return cleanup function
    return () => handlers.forEach(({ event, handler }) => ipcRenderer.removeListener(event, handler));
  },

  // Platform detection
  platform: process.platform,
  isElectron: true,
});

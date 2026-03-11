const { app, BrowserWindow, Menu, shell, ipcMain, dialog } = require('electron');
const path = require('path');

// Defer isDev so app is properly initialized first
let isDev = false;

let mainWindow;

function createWindow() {
  isDev = process.env.NODE_ENV === 'development' || !app.isPackaged;

  mainWindow = new BrowserWindow({
    width: 1440,
    height: 900,
    minWidth: 1024,
    minHeight: 700,
    backgroundColor: '#050A1C',
    titleBarStyle: 'hidden',
    titleBarOverlay: {
      color: '#030710',
      symbolColor: '#94a3b8',
      height: 36,
    },
    webPreferences: {
      nodeIntegration: false,
      contextIsolation: true,
      preload: path.join(__dirname, 'preload.cjs'),
    },
    icon: path.join(__dirname, '../public/icon.png'),
    show: false,
  });

  // Load the app
  if (isDev) {
    mainWindow.loadURL('http://localhost:5173');
    // DevTools only when explicitly requested via F12 in the View menu
  } else {
    mainWindow.loadFile(path.join(__dirname, '../dist/index.html'));
  }

  mainWindow.once('ready-to-show', () => {
    mainWindow.show();
    mainWindow.focus();
  });

  mainWindow.on('closed', () => {
    mainWindow = null;
  });

  // Allow microphone access
  mainWindow.webContents.session.setPermissionRequestHandler((webContents, permission, callback) => {
    const allowedPermissions = ['media', 'microphone', 'audioCapture'];
    if (allowedPermissions.includes(permission)) {
      callback(true);
    } else {
      callback(false);
    }
  });

  // Open external links in browser
  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    shell.openExternal(url);
    return { action: 'deny' };
  });

  buildMenu();
}

function buildMenu() {
  const template = [
    {
      label: 'TIWATON',
      submenu: [
        { label: 'About TIWATON AI Studio', click: () => showAbout() },
        { type: 'separator' },
        { label: 'Audio Settings', accelerator: 'CmdOrCtrl+,', click: () => mainWindow?.webContents.send('menu:show-settings') },
        { type: 'separator' },
        { label: 'Quit', accelerator: 'CmdOrCtrl+Q', role: 'quit' },
      ],
    },
    {
      label: 'Engine',
      submenu: [
        { label: 'Start / Stop Engine', accelerator: 'CmdOrCtrl+Return', click: () => mainWindow?.webContents.send('menu:toggle-live') },
        { label: 'Restart Engine', click: () => mainWindow?.webContents.send('menu:hard-reset') },
        { type: 'separator' },
        { label: 'Auto-Calibrate Gate', click: () => mainWindow?.webContents.send('menu:calibrate') },
        { label: 'Learn Mic Fingerprint', click: () => mainWindow?.webContents.send('menu:learn-mic') },
        { label: 'Capture Noise Profile', click: () => mainWindow?.webContents.send('menu:capture-noise') },
      ],
    },
    {
      label: 'View',
      submenu: [
        { label: 'Live Visualizer', accelerator: 'CmdOrCtrl+1', click: () => mainWindow?.webContents.send('menu:set-tab', 'live') },
        { label: 'Audio Editor', accelerator: 'CmdOrCtrl+2', click: () => mainWindow?.webContents.send('menu:set-tab', 'editor') },
        { type: 'separator' },
        { label: 'Toggle AI Bypass', accelerator: 'B', click: () => mainWindow?.webContents.send('menu:toggle-bypass') },
        { type: 'separator' },
        { label: 'Full Screen', accelerator: 'F11', role: 'togglefullscreen' },
        { label: 'Zoom In', accelerator: 'CmdOrCtrl+=', role: 'zoomIn' },
        { label: 'Zoom Out', accelerator: 'CmdOrCtrl+-', role: 'zoomOut' },
        { label: 'Reset Zoom', accelerator: 'CmdOrCtrl+0', role: 'resetZoom' },
        { type: 'separator' },
        { label: 'Developer Tools', accelerator: 'F12', click: () => mainWindow?.webContents.toggleDevTools() },
      ],
    },
    {
      label: 'Export',
      submenu: [
        { label: 'Export WAV Recording', click: () => mainWindow?.webContents.send('menu:export-wav') },
        { label: 'Export WebM Recording', click: () => mainWindow?.webContents.send('menu:export-webm') },
        { label: 'Export MP4 Session', click: () => mainWindow?.webContents.send('menu:export-mp4') },
        { label: 'Spectrum Snapshot (PNG)', click: () => mainWindow?.webContents.send('menu:snapshot') },
        { type: 'separator' },
        { label: 'Save Session Snapshot', accelerator: 'CmdOrCtrl+S', click: () => mainWindow?.webContents.send('menu:save-snapshot') },
      ],
    },
    {
      label: 'Window',
      submenu: [
        { role: 'minimize' },
        { role: 'zoom' },
        { type: 'separator' },
        { role: 'front' },
      ],
    },
  ];

  const menu = Menu.buildFromTemplate(template);
  Menu.setApplicationMenu(menu);
}

function showAbout() {
  dialog.showMessageBox(mainWindow, {
    type: 'info',
    title: 'TIWATON AI Studio',
    message: 'TIWATON AI Studio v1.1',
    detail: '26-node DSP pipeline with AI-powered voice isolation, HyperGate noise reduction, RNNoise WASM, and broadcast-ready audio processing.\n\nBuilt for churches, podcasters, and live streamers.',
    buttons: ['OK'],
    icon: path.join(__dirname, '../public/icon.png'),
  });
}

// Handle IPC from renderer for file save dialogs
ipcMain.handle('dialog:save', async (event, { defaultName, filters }) => {
  const result = await dialog.showSaveDialog(mainWindow, {
    defaultPath: defaultName,
    filters: filters || [{ name: 'All Files', extensions: ['*'] }],
  });
  return result;
});

app.whenReady().then(() => {
  createWindow();
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});

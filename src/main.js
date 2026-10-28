'use strict';
// Moulin: push the shortcut, speak, push it again. The text goes to the clipboard, and to the cursor if "paste" is on.
const {
  app, BrowserWindow, Menu, Notification, Tray, clipboard, globalShortcut, ipcMain, nativeImage, screen, session, shell, systemPreferences,
} = require('electron');
const { EventEmitter } = require('node:events');
const fs = require('node:fs');
const path = require('node:path');
const chatgpt = require('./chatgpt');
const { paste } = require('./paste');

const ROOT = path.join(__dirname, '..');
const IS_MAC = process.platform === 'darwin';
const WAYLAND = process.platform === 'linux' && process.env.XDG_SESSION_TYPE === 'wayland';
const option = (name) => process.argv.find((a) => a.startsWith(`--${name}=`))?.slice(name.length + 3);
const MODE = process.argv.includes('--e2e') ? 'e2e' : process.argv.includes('--capture') ? 'capture' : 'app';
// "Moulin --toggle" starts or stops a dictation. A system shortcut can run it (for example on Wayland).
const TOGGLE = process.argv.includes('--toggle');
const SAMPLE = option('sample') || path.join(ROOT, 'fixtures', 'fr-sample.wav');
const OUT_DIR = option('out') || path.join(ROOT, 'artifacts');
const ICON = path.join(ROOT, 'assets', 'icon.png');
const MAX_RECORDING_MS = 5 * 60_000; // Uploads of more than 326 s are not tested.
const DEFAULTS = { hotkey: 'F5', autoPaste: true, openAtLogin: true, microphoneId: '', showOverlay: true };

if (MODE !== 'app') {
  // Test modes: the microphone plays the sample WAV one time (no loop), and no permission prompt shows.
  app.commandLine.appendSwitch('use-fake-ui-for-media-stream');
  app.commandLine.appendSwitch('use-fake-device-for-media-stream');
  app.commandLine.appendSwitch('use-file-for-fake-audio-capture', `${SAMPLE}%noloop`);
  // The sandbox of the audio service blocks the read of the WAV file. The page renderers keep their sandbox.
  app.commandLine.appendSwitch('disable-features', 'AudioServiceSandbox');
}
// Global shortcuts on Wayland go through the desktop portal.
if (process.platform === 'linux') app.commandLine.appendSwitch('enable-features', 'GlobalShortcutsPortal');
// Windows shows the notifications of the app only when this ID is the ID of its Start menu shortcut.
// Keep it equal to build.appId in package.json. The packaged package.json has no "build" field, so the code cannot read it.
if (process.platform === 'win32') app.setAppUserModelId('app.moulin');

const events = new EventEmitter(); // The E2E test listens to these events.
const settingsFile = () => path.join(app.getPath('userData'), 'settings.json');
let settings = { ...DEFAULTS };
let tray = null;
let overlay = null;
let settingsWin = null;
let state = 'idle'; // idle, recording, transcribing, or error
let status = 'Starting';
let signedIn = false;
let hotkeyError = null;
let pending = null; // The last recording. The app keeps it until its text is in the clipboard.
let recent = []; // The last texts of this session, for the tray menu.
let stopAt = 0;
let maxTimer = null;
let spinTimer = null;
let hideTimer = null;

function loadSettings() {
  try {
    return { ...DEFAULTS, ...JSON.parse(fs.readFileSync(settingsFile(), 'utf8')) };
  } catch (err) {
    if (err.code !== 'ENOENT') console.error(`Cannot read ${settingsFile()}: ${err.message}. Moulin uses the default settings.`);
    return { ...DEFAULTS };
  }
}

function saveSettings() {
  const file = settingsFile();
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(`${file}.tmp`, `${JSON.stringify(settings, null, 2)}\n`);
  fs.renameSync(`${file}.tmp`, file);
}

const readyText = () => (settings.hotkey ? `Ready. Press ${settings.hotkey} to dictate.` : 'Ready. Use the menu or your system shortcut to dictate.');

// Tray images: black template images on macOS (the menu bar colors them), colored tiles on Windows and Linux.
const images = new Map();
function trayImage(name) {
  if (!images.has(name)) {
    const template = IS_MAC && (name === 'idle' || name.startsWith('busy'));
    const file = IS_MAC ? `mac-${name}${template ? 'Template' : ''}.png` : `tile-${name}.png`;
    images.set(name, nativeImage.createFromPath(path.join(ROOT, 'assets', 'tray', file)));
  }
  return images.get(name);
}

function setState(next, text) {
  state = next;
  status = text;
  console.log(`[${next}] ${text}`);
  if (tray) {
    clearInterval(spinTimer);
    if (next === 'transcribing') {
      let frame = 0;
      tray.setImage(trayImage('busy0'));
      spinTimer = setInterval(() => tray.setImage(trayImage(`busy${(frame = (frame + 1) % 4)}`)), 120);
    } else {
      tray.setImage(trayImage(next === 'recording' ? 'recording' : next === 'error' ? 'error' : 'idle'));
    }
    tray.setToolTip(`Moulin: ${text}`);
    tray.setContextMenu(buildMenu());
  }
  pushStatus();
  events.emit('state', { state: next, text });
}

function buildMenu() {
  const busy = state === 'recording' || state === 'transcribing';
  const shortcut = settings.hotkey && globalShortcut.isRegistered(settings.hotkey) ? { accelerator: settings.hotkey, registerAccelerator: false } : {};
  const short = (t) => (t.length > 50 ? `${t.slice(0, 50).trim()}…` : t);
  return Menu.buildFromTemplate([
    { label: status, enabled: false },
    { type: 'separator' },
    { label: state === 'recording' ? 'Stop dictation' : 'Start dictation', enabled: state !== 'transcribing', click: toggle, ...shortcut },
    ...(state === 'recording' ? [{ label: 'Cancel dictation', accelerator: 'Escape', registerAccelerator: false, click: cancelRecording }] : []),
    ...(pending && !busy ? [{ label: 'Send the last recording again', click: transcribe }] : []),
    {
      label: 'Recent dictations',
      submenu: recent.length ? recent.map((t) => ({ label: short(t), click: () => clipboard.writeText(t) })) : [{ label: 'Nothing yet', enabled: false }],
    },
    { type: 'separator' },
    ...(signedIn ? [] : [{ label: 'Sign in to ChatGPT…', enabled: !busy, click: signIn }]),
    { label: 'Settings…', click: openSettings },
    { type: 'separator' },
    { label: 'Quit Moulin', click: () => app.quit() },
  ]);
}

function createOverlay() {
  overlay = new BrowserWindow({
    width: 320,
    height: 60,
    show: false,
    frame: false,
    transparent: true,
    resizable: false,
    movable: false,
    minimizable: false,
    maximizable: false,
    fullscreenable: false,
    skipTaskbar: true,
    focusable: false,
    hasShadow: false,
    alwaysOnTop: true,
    ...(IS_MAC ? { type: 'panel' } : {}),
    webPreferences: {
      preload: path.join(__dirname, 'overlay', 'preload.js'),
      backgroundThrottling: false,
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      autoplayPolicy: 'no-user-gesture-required',
    },
  });
  overlay.setAlwaysOnTop(true, 'screen-saver');
  if (IS_MAC) overlay.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true });
  return overlay.loadFile(path.join(__dirname, 'overlay', 'overlay.html'));
}

// The page of a window, or null after Electron destroys the window (for example during the quit).
// A destroyed window throws "Object has been destroyed" when the code reads its webContents.
const pageOf = (win) => (win && !win.isDestroyed() ? win.webContents : null);

// The pop-up shows at the bottom center of the screen with the mouse pointer.
function setOverlay(view, text = '', hideAfterMs = 0) {
  clearTimeout(hideTimer);
  if (!pageOf(overlay)) return;
  overlay.webContents.send('overlay:state', { state: view, text });
  if (view === 'hidden' || !settings.showOverlay) {
    overlay.hide();
  } else if (!overlay.isVisible()) {
    const { workArea } = screen.getDisplayNearestPoint(screen.getCursorScreenPoint());
    const [width, height] = overlay.getSize();
    overlay.setPosition(Math.round(workArea.x + (workArea.width - width) / 2), Math.round(workArea.y + workArea.height - height - 28));
    overlay.showInactive();
  }
  if (hideAfterMs) hideTimer = setTimeout(() => setOverlay('hidden'), hideAfterMs);
  events.emit('overlay', { view, text, visible: overlay.isVisible() });
}

function notify(body) {
  if (Notification.isSupported()) new Notification({ title: 'Moulin', body }).show();
}

function fail(message, hideAfterMs = 4000) {
  setState('error', message);
  setOverlay('error', message, hideAfterMs);
  if (!settings.showOverlay) notify(message);
}

function registerEscape(on) {
  if (!on) return globalShortcut.unregister('Escape');
  try {
    globalShortcut.register('Escape', cancelRecording);
  } catch {
    // Escape stays free. The tray menu can still cancel.
  }
}

// macOS asks one time. A refusal needs a change in System Settings.
async function microphoneAllowed() {
  if (!IS_MAC || MODE !== 'app') return true;
  const access = systemPreferences.getMediaAccessStatus('microphone');
  if (access === 'granted') return true;
  if (access === 'not-determined') return systemPreferences.askForMediaAccess('microphone');
  shell.openExternal('x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone');
  return false;
}

async function startRecording() {
  if (state === 'recording' || state === 'transcribing') return;
  if (!signedIn) signedIn = (await chatgpt.start()).signedIn;
  if (!signedIn) {
    fail('Sign in to ChatGPT first.');
    openSettings();
    return;
  }
  if (!(await microphoneAllowed())) {
    fail('Allow Moulin in System Settings > Privacy & Security > Microphone.', 6000);
    return;
  }
  setState('recording', 'Recording');
  setOverlay('recording');
  pageOf(overlay)?.send('rec', { command: 'start', deviceId: settings.microphoneId });
  registerEscape(true);
  maxTimer = setTimeout(stopRecording, MAX_RECORDING_MS);
}

function stopRecording() {
  if (state !== 'recording') return;
  clearTimeout(maxTimer);
  registerEscape(false);
  stopAt = Date.now();
  setState('transcribing', 'Transcribing');
  setOverlay('transcribing', 'Transcribing');
  pageOf(overlay)?.send('rec', { command: 'stop' });
}

function cancelRecording() {
  if (state !== 'recording') return;
  clearTimeout(maxTimer);
  registerEscape(false);
  pageOf(overlay)?.send('rec', { command: 'cancel' });
  setState('idle', readyText());
  setOverlay('hidden');
}

function toggle() {
  if (state === 'recording') stopRecording();
  else startRecording();
}

// macOS needs the Accessibility permission to paste. Without it, the text stays in the clipboard.
async function pasteText() {
  if (IS_MAC && !systemPreferences.isTrustedAccessibilityClient(false)) {
    systemPreferences.isTrustedAccessibilityClient(true);
    notify('Allow Moulin in System Settings > Privacy & Security > Accessibility to paste. The text is in the clipboard.');
    return false;
  }
  try {
    await new Promise((resolve) => setTimeout(resolve, 60));
    await paste();
    return true;
  } catch (err) {
    notify(`Moulin cannot paste (${err.message}). The text is in the clipboard.`);
    return false;
  }
}

async function transcribe() {
  if (!pending) return;
  if (state !== 'transcribing') {
    setState('transcribing', 'Transcribing');
    setOverlay('transcribing', 'Transcribing');
  }
  const result = await chatgpt.transcribe(pending);
  const text = result.ok ? result.text.trim() : '';
  if (text) {
    try {
      await clipboard.writeText(result.text); // In Electron 45, the clipboard API returns promises.
    } catch (err) {
      Object.assign(result, { ok: false, message: `Moulin cannot write to the clipboard: ${err.message}` });
    }
  }
  if (result.ok) {
    result.clipboardAt = Date.now();
    pending = null;
    if (text) recent = [result.text, ...recent].slice(0, 5);
    const pasted = text && settings.autoPaste && MODE === 'app' ? await pasteText() : false;
    setState('idle', readyText());
    setOverlay('done', !text ? 'No speech' : pasted ? 'Pasted' : 'Copied', 1300);
  } else {
    if (result.status === 401) signedIn = false;
    fail(result.message);
    if (result.showWindow) chatgpt.show();
  }
  events.emit('result', { ...result, stopAt });
}

// An empty shortcut means no Moulin shortcut: a system shortcut runs "Moulin --toggle".
function registerHotkey(accelerator = settings.hotkey) {
  if (!accelerator) return null;
  try {
    if (globalShortcut.register(accelerator, toggle)) return null;
    return WAYLAND
      ? `This desktop refuses the shortcut ${accelerator}. Add a system shortcut that runs Moulin with --toggle, and remove this one.`
      : `${accelerator} is in use by another app. Choose a different shortcut.`;
  } catch {
    return `${accelerator} is not a valid shortcut.`;
  }
}

// Keep the old shortcut if the new one does not register.
function changeHotkey(next) {
  if (settings.hotkey) globalShortcut.unregister(settings.hotkey);
  const error = registerHotkey(next);
  if (error) {
    registerHotkey(settings.hotkey);
    return error;
  }
  settings.hotkey = next;
  return null;
}

function applyLoginItem() {
  if (!app.isPackaged || MODE !== 'app') return;
  if (process.platform !== 'linux') {
    app.setLoginItemSettings({ openAtLogin: settings.openAtLogin });
    return;
  }
  const file = path.join(app.getPath('home'), '.config', 'autostart', 'moulin.desktop');
  if (!settings.openAtLogin) {
    fs.rmSync(file, { force: true });
    return;
  }
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const exec = process.env.APPIMAGE || process.execPath;
  fs.writeFileSync(file, `[Desktop Entry]\nType=Application\nName=Moulin\nExec="${exec}"\nIcon=moulin\nX-GNOME-Autostart-enabled=true\n`);
}

async function signIn() {
  setState(state, 'Sign in to ChatGPT in the window');
  signedIn = await chatgpt.signIn();
  setState(signedIn ? 'idle' : 'error', signedIn ? readyText() : 'Not signed in to ChatGPT');
}

function snapshot() {
  return {
    settings,
    state,
    status,
    signedIn,
    hotkeyError,
    version: app.getVersion(),
    platform: process.platform,
    accessibility: IS_MAC ? systemPreferences.isTrustedAccessibilityClient(false) : true,
  };
}

function pushStatus() {
  if (settingsWin && !settingsWin.isDestroyed()) settingsWin.webContents.send('status', snapshot());
}

function openSettings() {
  if (MODE !== 'app') return;
  if (settingsWin && !settingsWin.isDestroyed()) {
    settingsWin.show();
    settingsWin.focus();
    app.focus({ steal: true });
    return;
  }
  settingsWin = new BrowserWindow({
    width: 560,
    height: 800,
    useContentSize: true,
    resizable: false,
    maximizable: false,
    fullscreenable: false,
    show: false,
    title: 'Moulin',
    icon: ICON,
    ...(IS_MAC ? { titleBarStyle: 'hiddenInset' } : {}),
    webPreferences: { preload: path.join(__dirname, 'settings', 'preload.js'), contextIsolation: true, nodeIntegration: false, sandbox: true },
  });
  settingsWin.webContents.setWindowOpenHandler(({ url }) => {
    if (url.startsWith('https://')) shell.openExternal(url);
    return { action: 'deny' };
  });
  settingsWin.once('ready-to-show', () => {
    settingsWin.show();
    app.focus({ steal: true });
  });
  settingsWin.on('closed', () => { settingsWin = null; });
  settingsWin.loadFile(path.join(__dirname, 'settings', 'settings.html'));
}

function listenToPages() {
  const fromOverlay = (event) => event.sender === pageOf(overlay);
  const fromSettings = (event) => event.sender === pageOf(settingsWin);

  // Only the pages of the app get the microphone, and only for audio.
  // Chromium also calls these handlers during the quit, after the pop-up window is destroyed.
  const ours = (wc) => !!wc && (wc === pageOf(overlay) || wc === pageOf(settingsWin));
  session.defaultSession.setPermissionRequestHandler((wc, permission, callback, details) =>
    callback(ours(wc) && permission === 'media' && (details.mediaTypes || []).every((t) => t === 'audio')));
  session.defaultSession.setPermissionCheckHandler((wc, permission) => ours(wc) && permission === 'media');

  ipcMain.on('rec:started', (e) => { if (fromOverlay(e)) events.emit('started'); });
  ipcMain.on('rec:error', (e, message) => {
    if (!fromOverlay(e)) return;
    clearTimeout(maxTimer);
    registerEscape(false);
    fail(`Microphone error: ${String(message)}`);
    events.emit('micError', String(message));
  });
  ipcMain.on('rec:audio', (e, bytes, mimeType, durationMs) => {
    if (!fromOverlay(e) || !(bytes instanceof Uint8Array) || typeof mimeType !== 'string') return;
    pending = { audio: Buffer.from(bytes), mimeType, durationMs: Number(durationMs) || 0 };
    events.emit('audio', pending);
    transcribe();
  });
  ipcMain.on('overlay:clicked', (e) => { if (fromOverlay(e) && state === 'recording') stopRecording(); });

  ipcMain.handle('settings:get', (e) => (fromSettings(e) ? snapshot() : null));
  ipcMain.handle('settings:set', (e, patch) => {
    if (!fromSettings(e) || !patch || typeof patch !== 'object') return null;
    if (typeof patch.hotkey === 'string' && patch.hotkey !== settings.hotkey) hotkeyError = changeHotkey(patch.hotkey);
    for (const key of ['autoPaste', 'openAtLogin', 'showOverlay']) if (typeof patch[key] === 'boolean') settings[key] = patch[key];
    if (typeof patch.microphoneId === 'string') settings.microphoneId = patch.microphoneId;
    saveSettings();
    applyLoginItem();
    // A good shortcut also clears an old error, for example "This desktop refuses the shortcut".
    if (state === 'idle' || (state === 'error' && signedIn && !hotkeyError)) setState('idle', readyText());
    return snapshot();
  });
  // While the user types a new shortcut, the old one must not start a dictation.
  ipcMain.on('settings:capture', (e, on) => {
    if (!fromSettings(e) || !settings.hotkey) return;
    if (on) globalShortcut.unregister(settings.hotkey);
    else if (!globalShortcut.isRegistered(settings.hotkey)) registerHotkey();
  });
  ipcMain.handle('account:sign-in', async (e) => {
    if (!fromSettings(e)) return null;
    await signIn();
    return snapshot();
  });
  ipcMain.handle('account:sign-out', async (e) => {
    if (!fromSettings(e)) return null;
    signedIn = (await chatgpt.signOut()).signedIn;
    setState('error', 'Not signed in to ChatGPT');
    return snapshot();
  });
  ipcMain.on('app:accessibility', (e) => {
    if (!fromSettings(e) || !IS_MAC) return;
    if (!systemPreferences.isTrustedAccessibilityClient(true)) {
      shell.openExternal('x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility');
    }
  });
}

// First launch on macOS: ask for the microphone and for Accessibility now, so that the first dictation records and pastes.
async function askPermissionsOnce() {
  if (!IS_MAC) return;
  await microphoneAllowed();
  if (settings.autoPaste) systemPreferences.isTrustedAccessibilityClient(true);
}

async function main() {
  const firstRun = !fs.existsSync(settingsFile());
  settings = loadSettings();
  // Windows and Linux: no File, Edit, and View menu bar in the windows. macOS keeps its menu for Cmd+C and Cmd+V.
  if (!IS_MAC) Menu.setApplicationMenu(null);
  if (MODE === 'capture') return require('./capture').run(chatgpt.create({ allowMedia: true, icon: ICON }), chatgpt, OUT_DIR);
  if (IS_MAC) app.dock?.hide();
  tray = new Tray(trayImage('idle'));
  if (process.platform === 'win32') tray.on('click', () => tray.popUpContextMenu());
  setState('idle', 'Starting');
  listenToPages();
  await createOverlay();
  chatgpt.create({ icon: ICON });
  hotkeyError = registerHotkey();
  applyLoginItem();
  const page = await chatgpt.start();
  signedIn = page.signedIn;
  if (page.status === 403) {
    setState('error', 'Cloudflare asks for a check. Complete it in the ChatGPT window.');
    chatgpt.show();
  } else if (!signedIn) {
    setState('error', 'Not signed in to ChatGPT');
    openSettings();
  } else if (hotkeyError) {
    setState('error', hotkeyError);
  } else {
    setState('idle', readyText());
  }
  if (MODE === 'app' && firstRun) {
    saveSettings();
    openSettings();
    await askPermissionsOnce();
  }
  if (MODE === 'app' && TOGGLE) toggle(); // A system shortcut started Moulin: start the dictation now.
  if (MODE === 'e2e') {
    require('./e2e').run({ events, settings, chatgpt, page, hotkeyOk: !hotkeyError, sample: SAMPLE, outDir: OUT_DIR });
  }
}

// A second start sends its request to the running Moulin: --toggle starts or stops a dictation, a plain start opens the settings.
if (!app.requestSingleInstanceLock({ toggle: TOGGLE })) {
  if (!TOGGLE) console.error('Moulin runs already.');
  app.exit(TOGGLE ? 0 : 1);
} else {
  app.on('second-instance', (event, argv, workingDirectory, request) => (request?.toggle ? toggle() : openSettings()));
  app.on('window-all-closed', () => {}); // Moulin stays in the tray.
  app.on('will-quit', () => globalShortcut.unregisterAll());
  app.whenReady().then(main).catch((err) => {
    console.error(err);
    app.exit(1);
  });
}

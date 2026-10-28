'use strict';
// The hidden chatgpt.com window: sign-in, light page, and the upload to /backend-api/transcribe.
const { app, BrowserWindow, session } = require('electron');
const { randomUUID } = require('node:crypto');

const ORIGIN = 'https://chatgpt.com';
const HOME = `${ORIGIN}/`;
const LIGHT = `${ORIGIN}/robots.txt`; // A small chatgpt.com document. The upload needs only the origin and the cookies.
const PARTITION = 'persist:chatgpt';
const WORLD = 1042; // The isolated world for the app code. The scripts of the page cannot see it.
const TIMEOUT_MS = 120_000;
const MESSAGES = {
  usage_limit_reached: 'You reached the ChatGPT dictation limit. Try again later.',
  usage_not_included: 'Your ChatGPT plan does not include dictation.',
};
const webPreferences = { partition: PARTITION, backgroundThrottling: false, contextIsolation: true, nodeIntegration: false, sandbox: true };

let win = null;
let kind = null; // 'light' or 'full': the document in the hidden window.
let quitting = false;
app.on('before-quit', () => { quitting = true; });

const originOf = (url) => { try { return new URL(url).origin; } catch { return ''; } };
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// The user agent of Chrome. Electron adds "Electron/x.y" and the app name, and some sign-in pages block them.
const PLATFORMS = { darwin: 'Macintosh; Intel Mac OS X 10_15_7', win32: 'Windows NT 10.0; Win64; x64', linux: 'X11; Linux x86_64' };
function chromeUserAgent() {
  const major = process.versions.chrome.split('.')[0];
  return `Mozilla/5.0 (${PLATFORMS[process.platform] || PLATFORMS.linux}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/${major}.0.0.0 Safari/537.36`;
}

function create({ allowMedia = false, icon } = {}) {
  const ses = session.fromPartition(PARTITION);
  ses.setUserAgent(chromeUserAgent());
  // The page gets no permission. In capture mode only, chatgpt.com gets the microphone for its own dictation button.
  const allowed = (permission, origin) => allowMedia && permission === 'media' && origin === ORIGIN;
  ses.setPermissionRequestHandler((_wc, permission, callback, details) => callback(allowed(permission, originOf(details.requestingUrl))));
  ses.setPermissionCheckHandler((_wc, permission, origin) => allowed(permission, originOf(origin)));
  win = new BrowserWindow({ show: false, width: 1000, height: 820, title: 'Sign in to ChatGPT - Moulin', icon, webPreferences });
  win.on('page-title-updated', (event) => event.preventDefault());
  // Pop-ups (for example a sign-in provider) open only while the user sees the window.
  win.webContents.setWindowOpenHandler(() => ({ action: win.isVisible() ? 'allow' : 'deny' }));
  win.on('close', (event) => {
    if (quitting) return;
    event.preventDefault();
    win.hide();
  });
  return win;
}

// Load a URL in the hidden window. Return the HTTP status of the document (0 if the load fails).
async function load(url) {
  let status = 0;
  const onNavigate = (_event, _url, code) => { status = code; };
  win.webContents.on('did-navigate', onNavigate);
  try {
    await win.webContents.loadURL(url);
  } catch {
    // A failed or replaced load. The status shows the result.
  } finally {
    win.webContents.off('did-navigate', onNavigate);
  }
  kind = url === LIGHT ? 'light' : 'full';
  return status;
}

function run(code) {
  return win.webContents.executeJavaScriptInIsolatedWorld(WORLD, [{ code }]);
}

// True if the page session has an access token. The token stays in the page.
async function isSignedIn() {
  if (originOf(win.webContents.getURL()) !== ORIGIN) return false;
  return run("fetch('/api/auth/session', { cache: 'no-store' }).then((r) => (r.ok ? r.json() : {})).then((s) => !!s.accessToken, () => false)")
    .catch(() => false);
}

// Open the light page. If Cloudflare blocks it, open the full page.
async function start() {
  let status = await load(LIGHT);
  if (status === 403) status = await load(HOME);
  return { kind, url: win.webContents.getURL(), status, signedIn: status === 200 && (await isSignedIn()) };
}

function show() {
  win.show();
  app.focus({ steal: true });
}

// Delete the cookies and the storage of chatgpt.com in the app. The light page then has no session.
async function signOut() {
  await session.fromPartition(PARTITION).clearStorageData();
  return start();
}

// Show chatgpt.com to the user. Resolve true when the session has a token, or false when the user closes the window.
function signIn() {
  return new Promise((resolve) => {
    const wc = win.webContents;
    let done = false;
    const finish = async (ok) => {
      if (done) return;
      done = true;
      wc.off('did-finish-load', check);
      wc.off('did-navigate-in-page', check);
      win.off('hide', onHide);
      if (ok) {
        win.hide();
        await start();
      }
      resolve(ok);
    };
    const check = async () => { if (await isSignedIn()) finish(true); };
    const onHide = () => finish(false);
    wc.on('did-finish-load', check);
    wc.on('did-navigate-in-page', check);
    win.on('hide', onHide);
    show();
    load(HOME);
  });
}

// The code runs in the isolated world of the page. It returns no secret: only the status, the text, and an error code.
// The form fields and the headers are the same as the request of the dictation button (capture of October 2026).
function uploadCode({ audio, mimeType, durationMs }) {
  return `(async () => {
  try {
    const s = await fetch('/api/auth/session', { cache: 'no-store' });
    const session = s.ok ? await s.json() : {};
    if (!session.accessToken) return { stage: 'session', status: s.ok ? 401 : s.status, signedOut: s.ok, cfMitigated: s.headers.get('cf-mitigated') };
    const bytes = Uint8Array.from(atob(${JSON.stringify(audio.toString('base64'))}), (c) => c.charCodeAt(0));
    const form = new FormData();
    form.append('file', new Blob([bytes], { type: ${JSON.stringify(mimeType)} }), 'codex.webm');
    form.append('dictation_session_id', ${JSON.stringify(randomUUID())});
    form.append('attempt_id', crypto.randomUUID());
    form.append('duration_ms', ${JSON.stringify(String(durationMs))});
    const headers = {
      authorization: 'Bearer ' + session.accessToken,
      'oai-language': navigator.language,
      originator: 'Codex Browser',
      'x-openai-web-frontend': 'codex_webview',
      'x-openai-codex-window-type': 'browser',
    };
    if (session.account?.id) headers['chatgpt-account-id'] = session.account.id;
    const did = document.cookie.match(/(?:^|; )oai-did=([^;]*)/);
    if (did) headers['oai-device-id'] = headers['oai-did'] = decodeURIComponent(did[1]);
    const r = await fetch('/backend-api/transcribe', { method: 'POST', headers, body: form, referrer: '/', signal: AbortSignal.timeout(${TIMEOUT_MS}) });
    const body = await r.text();
    let json = null;
    try { json = JSON.parse(body); } catch {}
    const out = { stage: 'transcribe', status: r.status, cfMitigated: r.headers.get('cf-mitigated') };
    if (r.ok && typeof json?.text === 'string') out.text = json.text;
    else out.code = ${JSON.stringify(Object.keys(MESSAGES))}.find((c) => body.includes(c))
      ?? json?.detail?.code ?? (typeof json?.detail === 'string' ? json.detail.slice(0, 200) : null);
    return out;
  } catch (e) {
    return { stage: 'network', status: 0, code: String(e?.name || e) };
  }
})()`;
}

function classify(r) {
  if (typeof r.text === 'string') return { ok: true, ...r };
  if (MESSAGES[r.code]) return { ok: false, retry: false, message: MESSAGES[r.code], ...r };
  if (r.status === 401) return { ok: false, retry: false, message: 'The ChatGPT session expired. Use "Sign in to ChatGPT".', ...r };
  if (r.status === 403 && r.cfMitigated) return { ok: false, retry: true, challenge: true, message: 'Cloudflare asks for a check. Complete it in the ChatGPT window.', ...r };
  if (r.status === 429) return { ok: false, retry: false, message: 'ChatGPT limits the requests (HTTP 429). Try again later.', ...r };
  return { ok: false, retry: r.status === 0 || r.status >= 500, message: `The transcription failed (${r.code || `HTTP ${r.status}`}).`, ...r };
}

// One upload, and a maximum of one automatic retry. A retry gets a new attempt_id.
async function transcribe(recording) {
  const code = uploadCode(recording);
  const once = async () => {
    if (originOf(win.webContents.getURL()) !== ORIGIN) await start();
    return run(code).then(classify, (e) => classify({ stage: 'page', status: 0, code: e.message }));
  };
  let result = { ...(await once()), attempts: 1 };
  if (!result.ok && result.retry) {
    if (result.challenge && kind !== 'light') return { ...result, showWindow: true };
    if (result.challenge) await load(HOME);
    result = { ...(await once()), attempts: 2, page: kind };
    if (result.challenge) result.showWindow = true;
  }
  return result;
}

// The working set of the renderer process of a page, in MB.
function memoryMB(wc = win.webContents) {
  const pid = wc.getOSProcessId();
  const metric = app.getAppMetrics().find((m) => m.pid === pid);
  return metric ? Math.round(metric.memory.workingSetSize / 1024) : null;
}

// Load the full chatgpt.com app in a second hidden window, and measure its renderer.
async function measureFullPage(settleMs = 8000) {
  const tmp = new BrowserWindow({ show: false, webPreferences });
  tmp.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  try {
    await tmp.webContents.loadURL(HOME).catch(() => {});
    await delay(settleMs);
    return { fullPage: memoryMB(tmp.webContents), sameProcess: tmp.webContents.getOSProcessId() === win.webContents.getOSProcessId() };
  } finally {
    tmp.destroy();
  }
}

module.exports = { ORIGIN, HOME, WORLD, create, load, start, show, signIn, signOut, transcribe, memoryMB, measureFullPage, kind: () => kind };

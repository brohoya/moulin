'use strict';
// Capture mode (ground truth): the user does one dictation with the button of the chatgpt.com page.
// The Chrome DevTools Protocol records the request. The capture file keeps names and shapes, and no secret value.
const { app } = require('electron');
const fs = require('node:fs');
const path = require('node:path');

const IS_TRANSCRIBE = /\/backend-api\/transcribe/;
// Headers with a value that is not a secret. The capture file redacts all other values.
const SAFE = /^(:method|:path|:scheme|:authority|accept|accept-encoding|accept-language|content-type|content-length|origin|priority|oai-language|oai-client-version|originator|x-openai-web-frontend|x-openai-codex-window-type|sec-ch-ua|sec-ch-ua-mobile|sec-ch-ua-platform|sec-fetch-dest|sec-fetch-mode|sec-fetch-site|user-agent|cf-mitigated|server)$/;
// The message box is the only contenteditable element of the page (October 2026).
const CLEAR_COMPOSER = `(() => {
  const box = document.querySelector('[contenteditable="true"]');
  if (!box || !box.textContent.trim()) return false;
  box.focus();
  document.execCommand('selectAll');
  document.execCommand('delete');
  return !box.textContent.trim();
})()`;

function redact(headers = {}) {
  return Object.fromEntries(Object.entries(headers).map(([name, value]) => {
    const key = name.toLowerCase();
    return [key, SAFE.test(key) ? String(value).replace(/boundary=[^;\s]+/, 'boundary=<boundary>') : '<redacted>'];
  }));
}

// The keys and the types of a JSON value, two levels deep. No values.
const shape = (v, depth = 2) => (Array.isArray(v) ? (depth ? [v.length ? shape(v[0], depth - 1) : 'empty'] : 'array')
  : v && typeof v === 'object' ? (depth ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, shape(x, depth - 1)])) : 'object') : typeof v);

// The parts of a multipart body: name, file name, and content type. A short text value stays (for example a language code).
function parseForm(body) {
  const parts = [];
  for (const m of body.matchAll(/Content-Disposition: form-data; name="([^"]*)"(?:; filename="([^"]*)")?\r\n(?:Content-Type: ([^\r\n]*)\r\n)?\r\n/gi)) {
    const part = { name: m[1], filename: m[2] ?? null, contentType: m[3] ?? null };
    if (part.filename === null) {
      const rest = body.slice(m.index + m[0].length);
      const value = rest.slice(0, rest.indexOf('\r\n'));
      part.value = /^[0-9a-f]{8}-[0-9a-f]{4}-/i.test(value) ? '<uuid>' : value.length <= 40 ? value : `<${value.length} characters>`;
    }
    parts.push(part);
  }
  return parts;
}

// Runs in the page. Finds where the page can read each header value. Returns the places, not the values.
async function pageSearch(targets, shape) {
  const found = {};
  const visit = (value, where) => {
    if (typeof value === 'string') {
      for (const [name, target] of Object.entries(targets)) if (target && value === target) (found[name] ||= []).push(where);
      if (/^[[{]/.test(value)) { try { visit(JSON.parse(value), where); } catch {} }
    } else if (value && typeof value === 'object') {
      for (const [key, v] of Object.entries(value)) visit(v, `${where}.${key}`);
    }
  };
  let session = {};
  try { session = await (await fetch('/api/auth/session', { cache: 'no-store' })).json(); } catch {}
  visit(session, 'GET /api/auth/session');
  for (const store of ['localStorage', 'sessionStorage']) {
    for (let i = 0; i < window[store].length; i++) visit(window[store].getItem(window[store].key(i)), `${store}[${window[store].key(i)}]`);
  }
  for (const part of document.cookie.split('; ')) {
    const i = part.indexOf('=');
    try { visit(decodeURIComponent(part.slice(i + 1)), `document.cookie ${part.slice(0, i)}`); } catch {}
  }
  return { found, sessionShape: shape(session) };
}

async function findSources(wc, world, headers) {
  const targets = {};
  for (const [name, value] of Object.entries(headers)) {
    const key = name.toLowerCase();
    if (key === 'authorization') targets[key] = String(value).replace(/^Bearer\s+/i, '');
    else if (/^(oai-|openai-|chatgpt-)/.test(key) && !SAFE.test(key)) targets[key] = String(value);
  }
  const found = Object.fromEntries(Object.keys(targets).map((key) => [key, []]));
  for (const cookie of await wc.session.cookies.get({ url: 'https://chatgpt.com' })) {
    let decoded = cookie.value;
    try { decoded = decodeURIComponent(cookie.value); } catch {}
    for (const [key, value] of Object.entries(targets)) {
      if (value && (cookie.value === value || decoded === value)) found[key].push(`cookie ${cookie.name}${cookie.httpOnly ? ' (HttpOnly)' : ''}`);
    }
  }
  const code = `(() => { const shape = ${shape}; return (${pageSearch})(${JSON.stringify(targets)}, shape); })()`;
  const page = await wc.executeJavaScriptInIsolatedWorld(world, [{ code }]);
  for (const [key, where] of Object.entries(page.found)) found[key].push(...where);
  return { found, sessionShape: page.sessionShape };
}

async function run(win, chatgpt, outDir) {
  const wc = win.webContents;
  const dbg = wc.debugger;
  const startedAt = new Date().toISOString();
  const report = { mode: 'capture', startedAt, electron: process.versions.electron, userAgent: wc.session.getUserAgent(), requests: [], websockets: [] };
  const pending = new Map(); // requestId -> transcribe request
  const extra = new Map(); // requestId -> raw headers on the wire (they can come before requestWillBeSent)
  let finished = false;

  const finish = async (reason) => {
    if (finished) return;
    finished = true;
    report.result = reason;
    report.fullPageMemoryMB = chatgpt.memoryMB();
    await new Promise((resolve) => setTimeout(resolve, 1500)); // The page puts the text in the box after its request.
    try { report.composerCleared = await wc.executeJavaScriptInIsolatedWorld(chatgpt.WORLD, [{ code: CLEAR_COMPOSER }]); } catch {}
    fs.mkdirSync(outDir, { recursive: true });
    const file = path.join(outDir, `capture-${startedAt.replace(/[:.]/g, '-')}.json`);
    fs.writeFileSync(file, `${JSON.stringify(report, null, 2)}\n`);
    console.log(`Capture: ${reason}. File: ${path.relative(process.cwd(), file)}`);
    setTimeout(() => app.quit(), 1500);
  };

  const readForm = async (requestId, request) => {
    try {
      const { postData, base64Encoded } = await dbg.sendCommand('Network.getRequestPostData', { requestId });
      return parseForm(base64Encoded ? Buffer.from(postData, 'base64').toString('latin1') : postData);
    } catch (err) {
      const raw = (request.postDataEntries || []).map((e) => Buffer.from(e.bytes || '', 'base64').toString('latin1')).join('');
      return raw ? parseForm(raw) : { error: err.message };
    }
  };

  const onMessage = async (method, params) => {
    const entry = pending.get(params.requestId);
    if (method === 'Network.webSocketCreated' && /dictation|transcri/i.test(params.url)) {
      report.websockets.push(params.url.split('?')[0]);
    } else if (method === 'Network.requestWillBeSentExtraInfo') {
      extra.set(params.requestId, params.headers);
      if (extra.size > 50) extra.delete(extra.keys().next().value);
    } else if (method === 'Network.requestWillBeSent' && IS_TRANSCRIBE.test(params.request.url)) {
      const record = { method: params.request.method, url: params.request.url.split('?')[0], headersFromPage: redact(params.request.headers) };
      pending.set(params.requestId, { record, headers: params.request.headers });
      record.form = await readForm(params.requestId, params.request);
    } else if (method === 'Network.responseReceived' && entry) {
      entry.record.response = { status: params.response.status, protocol: params.response.protocol, headers: redact(params.response.headers) };
    } else if (method === 'Network.loadingFinished' && entry) {
      pending.delete(params.requestId);
      const { record } = entry;
      const sent = extra.get(params.requestId) || {};
      record.headersOnTheWire = redact(sent);
      const { body, base64Encoded } = await dbg.sendCommand('Network.getResponseBody', { requestId: params.requestId });
      const text = base64Encoded ? Buffer.from(body, 'base64').toString('utf8') : body;
      let json = null;
      try { json = JSON.parse(text); } catch {}
      record.response.bodyShape = json ? shape(json) : `<${text.length} characters, not JSON>`;
      if (typeof json?.text === 'string') record.response.text = json.text; // The transcript of the sample WAV. It is not a secret.
      record.sources = await findSources(wc, chatgpt.WORLD, { ...entry.headers, ...sent });
      report.requests.push(record);
      finish('captured');
    } else if (method === 'Network.loadingFailed' && entry) {
      pending.delete(params.requestId);
      entry.record.failed = params.errorText;
      report.requests.push(entry.record);
      finish('the request failed');
    }
  };

  win.on('close', () => setImmediate(() => finish('the user closed the window')));
  setTimeout(() => finish('timeout'), 15 * 60_000);
  chatgpt.show();
  await chatgpt.load(chatgpt.HOME);
  // Attach after the first load: before it, the window has no page and Network.enable does not return.
  dbg.attach('1.3');
  dbg.on('message', (_event, method, params) => onMessage(method, params).catch((err) => console.error(`CDP ${method}: ${err.message}`)));
  await dbg.sendCommand('Network.enable');
  console.log('Capture: sign in, then do one dictation with the microphone button of the page. Do not send a message.');
}

module.exports = { run };

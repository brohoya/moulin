// The code of the capture mode in the chatgpt.com page (see capture.rs).
({
  // Runs at the start of each page of the window. Wraps fetch, and records the request to /backend-api/transcribe.
  record() {
    if (location.origin !== 'https://chatgpt.com') return;
    const records = (window.__moulinCapture = []);
    const original = window.fetch;
    window.fetch = async function (input, init) {
      const url = String(typeof input === 'string' || input instanceof URL ? input : input.url);
      if (!/\/backend-api\/transcribe/.test(url)) return original.apply(this, arguments);
      const record = { method: (init && init.method) || (input && input.method) || 'GET', url: url.split('?')[0], headers: {}, form: [] };
      new Headers((init && init.headers) || (input instanceof Request ? input.headers : undefined)).forEach((value, name) => { record.headers[name] = value; });
      const body = init && init.body;
      if (body instanceof FormData) {
        for (const [name, value] of body.entries()) {
          record.form.push(value instanceof Blob ? { name, filename: value.name ?? null, contentType: value.type || null } : { name, value: String(value) });
        }
      }
      try {
        const response = await original.apply(this, arguments);
        const headers = {};
        response.headers.forEach((value, name) => { headers[name] = value; });
        record.response = { status: response.status, headers, body: await response.clone().text() };
        records.push(record);
        return response;
      } catch (e) {
        record.failed = String(e);
        records.push(record);
        throw e;
      }
    };
  },

  // Gives the requests that record() kept, and empties the list.
  take() {
    return window.__moulinCapture ? window.__moulinCapture.splice(0) : [];
  },

  // Finds where the page can read each header value. Returns the places, not the values.
  async search(targets) {
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
    const shape = (v, depth = 2) => (Array.isArray(v) ? (depth ? [v.length ? shape(v[0], depth - 1) : 'empty'] : 'array')
      : v && typeof v === 'object' ? (depth ? Object.fromEntries(Object.entries(v).map(([k, x]) => [k, shape(x, depth - 1)])) : 'object') : typeof v);
    return { found, sessionShape: shape(session) };
  },

  // Empties the message box. The message box is the only contenteditable element of the page (October 2026).
  clearComposer() {
    const box = document.querySelector('[contenteditable="true"]');
    if (!box || !box.textContent.trim()) return false;
    box.focus();
    document.execCommand('selectAll');
    document.execCommand('delete');
    return !box.textContent.trim();
  },
})

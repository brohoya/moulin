// The upload of a recording in the chatgpt.com page (see upload.rs). It returns no secret: only the status, the text,
// and an error code. The form fields and the headers are the same as the request of the dictation button (capture of
// October 2026). The file is WAV instead of the WebM of the page: Moulin records the audio itself.
({
  async upload({ audio, sessionId, durationMs, codes, timeoutMs }) {
    try {
      const s = await fetch('/api/auth/session', { cache: 'no-store' });
      const session = s.ok ? await s.json() : {};
      if (!session.accessToken) return { stage: 'session', status: s.ok ? 401 : s.status, signedOut: s.ok, cfMitigated: s.headers.get('cf-mitigated') };
      // A loop into one array: Uint8Array.from on the text first makes a list with one item for each byte.
      const text = atob(audio);
      const bytes = new Uint8Array(text.length);
      for (let i = 0; i < text.length; i++) bytes[i] = text.charCodeAt(i);
      const form = new FormData();
      form.append('file', new Blob([bytes], { type: 'audio/wav' }), 'codex.wav');
      form.append('dictation_session_id', sessionId);
      form.append('attempt_id', crypto.randomUUID());
      form.append('duration_ms', durationMs);
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
      const r = await fetch('/backend-api/transcribe', { method: 'POST', headers, body: form, referrer: '/', signal: AbortSignal.timeout(timeoutMs) });
      const body = await r.text();
      let json = null;
      try { json = JSON.parse(body); } catch {}
      const out = { stage: 'transcribe', status: r.status, cfMitigated: r.headers.get('cf-mitigated') };
      if (r.ok && typeof json?.text === 'string') out.text = json.text;
      else out.code = codes.find((c) => body.includes(c))
        ?? json?.detail?.code ?? (typeof json?.detail === 'string' ? json.detail.slice(0, 200) : null);
      return out;
    } catch (e) {
      return { stage: 'network', status: 0, code: String(e?.name || e) };
    }
  },
})

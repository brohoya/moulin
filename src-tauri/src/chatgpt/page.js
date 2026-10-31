// Checks of the chatgpt.com page (see mod.rs). They return no secret.
({
  // The HTTP status of the document, because WebKit does not give it. robots.txt is plain text, and a Cloudflare
  // check is HTML.
  status(light) {
    if (light && document.contentType === 'text/plain') return 200;
    return fetch(location.href, { method: 'HEAD', cache: 'no-store' }).then((r) => r.status, () => 0);
  },

  // True if the page session has an access token. The token stays in the page.
  signedIn() {
    return fetch('/api/auth/session', { cache: 'no-store' }).then((r) => (r.ok ? r.json() : {})).then((s) => !!s.accessToken, () => false);
  },
})

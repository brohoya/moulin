// The hidden chatgpt.com window: sign-in, light page, and the upload to /backend-api/transcribe.
use crate::audio::Recording;
use base64::Engine;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{
    AppHandle, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
    webview::{NewWindowResponse, PageLoadEvent, PermissionKind, PermissionResponse},
};
use tokio::sync::{oneshot, watch};

pub const ORIGIN: &str = "https://chatgpt.com";
pub const HOME: &str = "https://chatgpt.com/";
const LIGHT: &str = "https://chatgpt.com/robots.txt"; // A small chatgpt.com document. The upload needs only the origin and the cookies.
const LABEL: &str = "chatgpt";
const TIMEOUT_MS: u64 = 120_000;
/// WebKit reports no failed load, so a load that does not finish in this time counts as failed.
const LOAD_TIMEOUT: Duration = Duration::from_secs(20);
/// The session of chatgpt.com: one cookie, or several cookies with a suffix (".0", ".1") when it is long.
const SESSION_COOKIE: &str = "__Secure-next-auth.session-token";
const MESSAGES: [(&str, &str); 2] = [
    ("usage_limit_reached", "You reached the ChatGPT dictation limit. Try again later."),
    ("usage_not_included", "Your ChatGPT plan does not include dictation."),
];
/// The website data store of the window on macOS: it keeps the chatgpt.com session apart from the pages of Moulin.
/// WebKit has stores with an identifier on macOS 14 or later. On macOS 13, the window uses the default store of the app.
#[cfg(target_os = "macos")]
const STORE: [u8; 16] = *b"moulin chatgpt 1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Light,
    Full,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub kind: Option<Kind>,
    pub url: String,
    pub status: u16,
    pub signed_in: bool,
}

/// The result of an upload.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub ok: bool,
    pub stage: String,
    pub status: u16,
    pub text: Option<String>,
    pub code: Option<String>,
    pub cf_mitigated: Option<String>,
    pub message: String,
    pub retry: bool,
    pub challenge: bool,
    pub show_window: bool,
    pub attempts: u32,
    pub page: Option<Kind>,
}

pub struct ChatGpt {
    kind: Mutex<Option<Kind>>,
    /// The number of finished page loads.
    loads: watch::Sender<u64>,
    /// The number of times that the user closed the window.
    closes: watch::Sender<u64>,
    quitting: AtomicBool,
    next_id: AtomicU64,
    /// The name of the page property that keeps the results of the code of Moulin, as a JSON string.
    key: String,
}

impl ChatGpt {
    pub fn new() -> Self {
        let key = format!("__moulin_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
        Self {
            kind: Mutex::new(None),
            loads: watch::Sender::new(0),
            closes: watch::Sender::new(0),
            quitting: AtomicBool::new(false),
            next_id: AtomicU64::new(1),
            key: serde_json::to_string(&key).unwrap_or_default(),
        }
    }

    pub fn kind(&self) -> Option<Kind> {
        *self.kind.lock().unwrap()
    }

    pub fn quit(&self) {
        self.quitting.store(true, Ordering::Relaxed);
    }
}

fn origin_of(url: Option<Url>) -> String {
    url.map(|u| u.origin().ascii_serialization()).unwrap_or_default()
}

fn window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window(LABEL).ok_or_else(|| "the ChatGPT window does not exist".to_string())
}

fn page_url(app: &AppHandle) -> Option<Url> {
    window(app).ok()?.url().ok()
}

/// Creates the hidden window. The page gets no permission. In capture mode only, chatgpt.com gets the microphone
/// for its own dictation button, and `script` runs at the start of each page.
pub fn create(app: &AppHandle, allow_media: bool, script: Option<&str>) -> tauri::Result<WebviewWindow> {
    let blank: Url = "about:blank".parse().expect("a valid URL");
    let handle = app.clone();
    let mut builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(blank))
        .title("Sign in to ChatGPT - Moulin")
        .inner_size(1000.0, 820.0)
        .visible(false)
        .on_permission_request(move |_, kind| match kind {
            PermissionKind::Microphone if allow_media => PermissionResponse::Allow,
            _ => PermissionResponse::Deny,
        })
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Finished {
                webview.app_handle().state::<ChatGpt>().loads.send_modify(|n| *n += 1);
            }
        })
        // Pop-ups (for example a sign-in provider) open only while the user sees the window.
        .on_new_window(move |_url, _features| {
            let visible = handle.get_webview_window(LABEL).and_then(|w| w.is_visible().ok()).unwrap_or(false);
            if visible { NewWindowResponse::Allow } else { NewWindowResponse::Deny }
        });
    if let Some(script) = script {
        builder = builder.initialization_script(script);
    }
    #[cfg(target_os = "macos")]
    {
        builder = builder.user_agent(&crate::platform::safari_user_agent()).data_store_identifier(STORE);
    }
    // Windows and Linux: a separate data folder keeps the chatgpt.com session apart from the pages of Moulin.
    #[cfg(not(target_os = "macos"))]
    {
        builder = builder.data_directory(app.path().app_local_data_dir()?.join("chatgpt"));
    }
    let created = builder.build()?;
    let handle = app.clone();
    created.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            let me = handle.state::<ChatGpt>();
            if me.quitting.load(Ordering::Relaxed) {
                return;
            }
            api.prevent_close();
            if let Ok(w) = window(&handle) {
                let _ = w.hide();
            }
            me.closes.send_modify(|n| *n += 1);
        }
    });
    Ok(created)
}

/// Runs code in the page and returns its value. The code is an expression, and it can give a promise.
/// The page keeps the result under a random property until Moulin reads it: `eval` gives no result for a promise.
/// The code returns no secret: only a status, a text, or an error code.
async fn run(app: &AppHandle, code: &str, limit: Duration) -> Result<Value, String> {
    let me = app.state::<ChatGpt>();
    let w = window(app)?;
    let id = me.next_id.fetch_add(1, Ordering::Relaxed);
    let key = &me.key;
    w.eval(format!(
        "(() => {{ const box = window[{key}] || (window[{key}] = {{}}); box[{id}] = null; \
         Promise.resolve().then(() => ({code})).then(\
         (value) => {{ box[{id}] = JSON.stringify({{ value: value ?? null }}); }}, \
         (error) => {{ box[{id}] = JSON.stringify({{ error: String((error && error.message) || error) }}); }}); }})()"
    ))
    .map_err(|err| err.to_string())?;
    let poll = format!(
        "(() => {{ const box = window[{key}]; if (!box || !({id} in box)) return false; \
         const value = box[{id}]; if (value !== null) delete box[{id}]; return value; }})()"
    );
    let deadline = Instant::now() + limit;
    loop {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let (tx, rx) = oneshot::channel();
        let tx = Mutex::new(Some(tx));
        w.eval_with_callback(poll.clone(), move |json| {
            if let Some(tx) = tx.lock().unwrap().take() {
                let _ = tx.send(json);
            }
        })
        .map_err(|err| err.to_string())?;
        if let Ok(Ok(json)) = tokio::time::timeout(Duration::from_secs(5), rx).await {
            match serde_json::from_str::<Value>(&json) {
                Ok(Value::String(text)) => {
                    let result: Value = serde_json::from_str(&text).map_err(|err| err.to_string())?;
                    return match result.get("error").and_then(Value::as_str) {
                        Some(error) => Err(error.to_string()),
                        None => Ok(result.get("value").cloned().unwrap_or(Value::Null)),
                    };
                }
                // The page changed (a navigation), so the result is lost.
                Ok(Value::Bool(false)) => return Err("the page changed during the request".into()),
                _ => {}
            }
        }
        if Instant::now() > deadline {
            return Err("the page did not answer".into());
        }
    }
}

/// Load a URL in the hidden window. Return the HTTP status of the document (0 if the load fails).
pub async fn load(app: &AppHandle, url: &str) -> u16 {
    let me = app.state::<ChatGpt>();
    let Ok(w) = window(app) else { return 0 };
    let mut loads = me.loads.subscribe();
    loads.borrow_and_update();
    *me.kind.lock().unwrap() = Some(if url == LIGHT { Kind::Light } else { Kind::Full });
    if w.navigate(url.parse().expect("a valid URL")).is_err() {
        return 0;
    }
    if tokio::time::timeout(LOAD_TIMEOUT, loads.changed()).await.is_err() || origin_of(page_url(app)) != ORIGIN {
        return 0;
    }
    // WebKit does not give the HTTP status of the document. robots.txt is plain text, and a Cloudflare check is HTML.
    let head = "fetch(location.href, { method: 'HEAD', cache: 'no-store' }).then((r) => r.status, () => 0)";
    let code = if url == LIGHT { format!("document.contentType === 'text/plain' ? 200 : {head}") } else { head.to_string() };
    run(app, &code, Duration::from_secs(30)).await.ok().and_then(|v| v.as_u64()).unwrap_or(0) as u16
}

/// True if the page session has an access token. The token stays in the page.
async fn is_signed_in(app: &AppHandle) -> bool {
    if origin_of(page_url(app)) != ORIGIN {
        return false;
    }
    let code =
        "fetch('/api/auth/session', { cache: 'no-store' }).then((r) => (r.ok ? r.json() : {})).then((s) => !!s.accessToken, () => false)";
    run(app, code, Duration::from_secs(30)).await.ok().and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Open the light page. If Cloudflare blocks it, open the full page.
pub async fn start(app: &AppHandle) -> Page {
    let mut status = load(app, LIGHT).await;
    if status == 403 {
        status = load(app, HOME).await;
    }
    let signed_in = status == 200 && is_signed_in(app).await;
    let kind = app.state::<ChatGpt>().kind();
    Page { kind, url: page_url(app).map(String::from).unwrap_or_default(), status, signed_in }
}

pub fn show(app: &AppHandle) {
    if let Ok(w) = window(app) {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Delete the cookies and the storage of chatgpt.com in the app. The light page then has no session.
pub async fn sign_out(app: &AppHandle) -> Page {
    if let Ok(w) = window(app) {
        let _ = w.clear_all_browsing_data();
        // The webview removes the data in the background. Wait until the session cookie is gone.
        let home: Url = HOME.parse().expect("a valid URL");
        for _ in 0..50 {
            match w.cookies_for_url(home.clone()) {
                Ok(cookies) if !cookies.iter().any(|c| c.name().starts_with(SESSION_COOKIE)) => break,
                _ => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
    }
    start(app).await
}

/// Show chatgpt.com to the user. Resolve true when the session has a token, or false when the user closes the window.
/// The page checks the session after each load, and every 3 seconds for the sign-ins that do not load a new page.
pub async fn sign_in(app: &AppHandle) -> bool {
    let me = app.state::<ChatGpt>();
    let Ok(w) = window(app) else { return false };
    let mut closes = me.closes.subscribe();
    closes.borrow_and_update();
    let mut loads = me.loads.subscribe();
    loads.borrow_and_update();
    show(app);
    let _ = w.navigate(HOME.parse().expect("a valid URL"));
    loop {
        tokio::select! {
            _ = closes.changed() => return false,
            _ = loads.changed() => {}
            _ = tokio::time::sleep(Duration::from_secs(3)) => {}
        }
        if is_signed_in(app).await {
            let _ = w.hide();
            start(app).await;
            return true;
        }
    }
}

/// The code for the page. It returns no secret: only the status, the text, and an error code.
/// The form fields and the headers are the same as the request of the dictation button (capture of October 2026).
/// The file is WAV instead of the WebM of the page: Moulin records the audio itself.
fn upload_code(recording: &Recording, session_id: &str) -> String {
    let audio = base64::engine::general_purpose::STANDARD.encode(&recording.wav);
    let codes: Vec<&str> = MESSAGES.iter().map(|(code, _)| *code).collect();
    format!(
        r#"(async () => {{
  try {{
    const s = await fetch('/api/auth/session', {{ cache: 'no-store' }});
    const session = s.ok ? await s.json() : {{}};
    if (!session.accessToken) return {{ stage: 'session', status: s.ok ? 401 : s.status, signedOut: s.ok, cfMitigated: s.headers.get('cf-mitigated') }};
    const bytes = Uint8Array.from(atob({audio}), (c) => c.charCodeAt(0));
    const form = new FormData();
    form.append('file', new Blob([bytes], {{ type: 'audio/wav' }}), 'codex.wav');
    form.append('dictation_session_id', {session_id});
    form.append('attempt_id', crypto.randomUUID());
    form.append('duration_ms', {duration});
    const headers = {{
      authorization: 'Bearer ' + session.accessToken,
      'oai-language': navigator.language,
      originator: 'Codex Browser',
      'x-openai-web-frontend': 'codex_webview',
      'x-openai-codex-window-type': 'browser',
    }};
    if (session.account?.id) headers['chatgpt-account-id'] = session.account.id;
    const did = document.cookie.match(/(?:^|; )oai-did=([^;]*)/);
    if (did) headers['oai-device-id'] = headers['oai-did'] = decodeURIComponent(did[1]);
    const r = await fetch('/backend-api/transcribe', {{ method: 'POST', headers, body: form, referrer: '/', signal: AbortSignal.timeout({TIMEOUT_MS}) }});
    const body = await r.text();
    let json = null;
    try {{ json = JSON.parse(body); }} catch {{}}
    const out = {{ stage: 'transcribe', status: r.status, cfMitigated: r.headers.get('cf-mitigated') }};
    if (r.ok && typeof json?.text === 'string') out.text = json.text;
    else out.code = {codes}.find((c) => body.includes(c))
      ?? json?.detail?.code ?? (typeof json?.detail === 'string' ? json.detail.slice(0, 200) : null);
    return out;
  }} catch (e) {{
    return {{ stage: 'network', status: 0, code: String(e?.name || e) }};
  }}
}})()"#,
        audio = serde_json::to_string(&audio).unwrap_or_default(),
        session_id = serde_json::to_string(session_id).unwrap_or_default(),
        duration = serde_json::to_string(&recording.duration_ms.to_string()).unwrap_or_default(),
        codes = serde_json::to_string(&codes).unwrap_or_default(),
    )
}

fn classify(raw: &Value) -> Outcome {
    let text = |key: &str| raw.get(key).and_then(Value::as_str).map(String::from);
    let mut out = Outcome {
        stage: text("stage").unwrap_or_default(),
        status: raw.get("status").and_then(Value::as_u64).unwrap_or(0) as u16,
        text: text("text"),
        code: text("code"),
        cf_mitigated: text("cfMitigated"),
        ..Outcome::default()
    };
    let known = out.code.as_deref().and_then(|code| MESSAGES.iter().find(|(c, _)| *c == code)).map(|(_, m)| *m);
    if out.text.is_some() {
        out.ok = true;
    } else if let Some(message) = known {
        out.message = message.into();
    } else if out.status == 401 {
        out.message = "The ChatGPT session expired. Use \"Sign in to ChatGPT\".".into();
    } else if out.status == 403 && out.cf_mitigated.is_some() {
        out.retry = true;
        out.challenge = true;
        out.message = "Cloudflare asks for a check. Complete it in the ChatGPT window.".into();
    } else if out.status == 429 {
        out.message = "ChatGPT limits the requests (HTTP 429). Try again later.".into();
    } else {
        out.retry = out.status == 0 || out.status >= 500;
        let reason = out.code.clone().unwrap_or_else(|| format!("HTTP {}", out.status));
        out.message = format!("The transcription failed ({reason}).");
    }
    out
}

/// One upload, and a maximum of one automatic retry. A retry gets a new attempt_id.
pub async fn transcribe(app: &AppHandle, recording: &Recording) -> Outcome {
    let code = upload_code(recording, &uuid::Uuid::new_v4().to_string());
    let once = || async {
        if origin_of(page_url(app)) != ORIGIN {
            start(app).await;
        }
        match run(app, &code, Duration::from_millis(TIMEOUT_MS + 10_000)).await {
            Ok(raw) => classify(&raw),
            Err(err) => classify(&json!({ "stage": "page", "status": 0, "code": err })),
        }
    };
    let mut result = Outcome { attempts: 1, ..once().await };
    if !result.ok && result.retry {
        let kind = app.state::<ChatGpt>().kind();
        if result.challenge && kind != Some(Kind::Light) {
            result.show_window = true;
            return result;
        }
        if result.challenge {
            load(app, HOME).await;
        }
        result = Outcome { attempts: 2, page: app.state::<ChatGpt>().kind(), ..once().await };
        result.show_window = result.challenge;
    }
    result
}

/// Runs code in the page of the window (for the capture mode).
pub async fn evaluate(app: &AppHandle, code: &str, limit: Duration) -> Result<Value, String> {
    run(app, code, limit).await
}

/// Changes each time that the user closes the window.
pub fn closes(app: &AppHandle) -> watch::Receiver<u64> {
    let mut closes = app.state::<ChatGpt>().closes.subscribe();
    closes.borrow_and_update();
    closes
}

/// The cookies of chatgpt.com in the window, with their values (for the capture mode).
pub fn cookies(app: &AppHandle) -> Vec<tauri::webview::Cookie<'static>> {
    let home: Url = HOME.parse().expect("a valid URL");
    window(app).ok().and_then(|w| w.cookies_for_url(home).ok()).unwrap_or_default()
}

/// The memory of the web content process of the page, in MB (macOS only).
pub async fn memory_mb(app: &AppHandle) -> Option<u64> {
    crate::platform::web_memory_mb(&window(app).ok()?).await
}

/// Load the full chatgpt.com app in a second hidden window, and measure its web content process (macOS only).
pub async fn measure_full_page(app: &AppHandle, settle: Duration) -> Value {
    if !cfg!(target_os = "macos") {
        return json!({ "fullPage": null });
    }
    let blank: Url = "about:blank".parse().expect("a valid URL");
    let builder = WebviewWindowBuilder::new(app, "chatgpt-measure", WebviewUrl::External(blank))
        .visible(false)
        .on_permission_request(|_, _| PermissionResponse::Deny)
        .on_new_window(|_, _| NewWindowResponse::Deny);
    #[cfg(target_os = "macos")]
    let builder = builder.user_agent(&crate::platform::safari_user_agent()).data_store_identifier(STORE);
    let Ok(tmp) = builder.build() else { return Value::Null };
    let _ = tmp.navigate(HOME.parse().expect("a valid URL"));
    tokio::time::sleep(settle).await;
    let full = crate::platform::web_memory_mb(&tmp).await;
    let _ = tmp.destroy();
    json!({ "fullPage": full })
}

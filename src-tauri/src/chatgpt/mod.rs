//! The hidden chatgpt.com window: the sign-in, the light page, the upload (see upload.rs), and the Cloudflare check
//! (see check.rs).
mod bridge;
pub mod check;
mod upload;

pub use bridge::evaluate;
pub use upload::{Outcome, fake_refusal, transcribe};

use serde::Serialize;
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{
    AppHandle, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
    webview::{Cookie, NewWindowResponse, PageLoadEvent, PermissionKind, PermissionResponse},
};
use tokio::sync::watch;

const ORIGIN: &str = "https://chatgpt.com";
pub const HOME: &str = "https://chatgpt.com/";
/// A small chatgpt.com document. The upload needs only the origin and the cookies.
const LIGHT: &str = "https://chatgpt.com/robots.txt";
const LABEL: &str = "chatgpt";
/// The title of the window for the sign-in. The Cloudflare check gives it another title.
const SIGN_IN_TITLE: &str = "Sign in to ChatGPT - Moulin";
/// WebKit reports no failed load, so a load that does not finish in this time counts as failed.
const LOAD_TIMEOUT: Duration = Duration::from_secs(20);
/// The session of chatgpt.com: one cookie, or several cookies with a suffix (".0", ".1") when it is long.
const SESSION_COOKIE: &str = "__Secure-next-auth.session-token";
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

/// The page in the window after a load.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub kind: Option<Kind>,
    pub url: String,
    pub status: u16,
    pub signed_in: bool,
}

/// The state of the window, for Tauri to keep.
pub struct ChatGpt {
    kind: Mutex<Option<Kind>>,
    /// The number of finished page loads.
    loads: watch::Sender<u64>,
    /// The number of times that the user closed the window.
    closes: watch::Sender<u64>,
    quitting: AtomicBool,
}

impl ChatGpt {
    pub fn new() -> Self {
        Self { kind: Mutex::new(None), loads: watch::Sender::new(0), closes: watch::Sender::new(0), quitting: AtomicBool::new(false) }
    }
}

fn state(app: &AppHandle) -> &ChatGpt {
    app.state::<ChatGpt>().inner()
}

/// The kind of the last page that Moulin loaded in the window.
fn kind(app: &AppHandle) -> Option<Kind> {
    *state(app).kind.lock().unwrap()
}

/// Lets the window close: Moulin quits.
pub fn quit(app: &AppHandle) {
    state(app).quitting.store(true, Ordering::Relaxed);
}

fn parse(url: &str) -> Url {
    url.parse().expect("a valid URL")
}

fn window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window(LABEL).ok_or_else(|| "the ChatGPT window does not exist".to_string())
}

fn page_url(app: &AppHandle) -> Option<Url> {
    window(app).ok()?.url().ok()
}

/// True if the window shows a page of chatgpt.com.
fn on_chatgpt(app: &AppHandle) -> bool {
    page_url(app).is_some_and(|url| url.origin().ascii_serialization() == ORIGIN)
}

/// Creates the hidden window. The page gets no permission. In capture mode only, chatgpt.com gets the microphone
/// for its own dictation button, and `script` runs at the start of each page.
pub fn create(app: &AppHandle, allow_media: bool, script: Option<&str>) -> tauri::Result<()> {
    let handle = app.clone();
    let mut builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(parse("about:blank")))
        .title(SIGN_IN_TITLE)
        .inner_size(1000.0, 820.0)
        .visible(false)
        .on_permission_request(move |_, kind| match kind {
            PermissionKind::Microphone if allow_media => PermissionResponse::Allow,
            _ => PermissionResponse::Deny,
        })
        .on_page_load(|webview, payload| {
            if payload.event() == PageLoadEvent::Finished {
                state(webview.app_handle()).loads.send_modify(|n| *n += 1);
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
    let app = app.clone();
    // The close button only hides the window: the uploads need its page.
    created.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event
            && !state(&app).quitting.load(Ordering::Relaxed)
        {
            api.prevent_close();
            if let Ok(w) = window(&app) {
                let _ = w.hide();
            }
            state(&app).closes.send_modify(|n| *n += 1);
        }
    });
    Ok(())
}

/// Loads a URL in the hidden window. Returns the HTTP status of the document (0 if the load fails).
pub async fn load(app: &AppHandle, url: &str) -> u16 {
    let Ok(w) = window(app) else { return 0 };
    let mut loads = state(app).loads.subscribe();
    loads.borrow_and_update();
    *state(app).kind.lock().unwrap() = Some(if url == LIGHT { Kind::Light } else { Kind::Full });
    if w.navigate(parse(url)).is_err() {
        return 0;
    }
    if tokio::time::timeout(LOAD_TIMEOUT, loads.changed()).await.is_err() || !on_chatgpt(app) {
        return 0;
    }
    // WebKit does not give the HTTP status of the document. robots.txt is plain text, and a Cloudflare check is HTML.
    let head = "fetch(location.href, { method: 'HEAD', cache: 'no-store' }).then((r) => r.status, () => 0)";
    let code = if url == LIGHT { format!("document.contentType === 'text/plain' ? 200 : {head}") } else { head.to_string() };
    evaluate(app, &code, Duration::from_secs(30)).await.ok().and_then(|v| v.as_u64()).unwrap_or(0) as u16
}

/// True if the page session has an access token. The token stays in the page.
async fn is_signed_in(app: &AppHandle) -> bool {
    if !on_chatgpt(app) {
        return false;
    }
    let code =
        "fetch('/api/auth/session', { cache: 'no-store' }).then((r) => (r.ok ? r.json() : {})).then((s) => !!s.accessToken, () => false)";
    evaluate(app, code, Duration::from_secs(30)).await.ok().and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Opens the light page. If Cloudflare blocks it, opens the full page.
pub async fn start(app: &AppHandle) -> Page {
    let mut status = load(app, LIGHT).await;
    if status == 403 {
        status = load(app, HOME).await;
    }
    let signed_in = status == 200 && is_signed_in(app).await;
    Page { kind: kind(app), url: page_url(app).map(String::from).unwrap_or_default(), status, signed_in }
}

pub fn show(app: &AppHandle) {
    if let Ok(w) = window(app) {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Deletes the cookies and the storage of chatgpt.com in the app. The light page then has no session.
pub async fn sign_out(app: &AppHandle) -> Page {
    if let Ok(w) = window(app) {
        let _ = w.clear_all_browsing_data();
        // The webview removes the data in the background. Wait until the session cookie is gone.
        for _ in 0..50 {
            match w.cookies_for_url(parse(HOME)) {
                Ok(cookies) if !cookies.iter().any(|c| c.name().starts_with(SESSION_COOKIE)) => break,
                _ => tokio::time::sleep(Duration::from_millis(100)).await,
            }
        }
    }
    start(app).await
}

/// Shows chatgpt.com to the user. Resolves true when the session has a token, or false when the user closes the window.
/// The page checks the session after each load, and every 3 seconds for the sign-ins that do not load a new page.
pub async fn sign_in(app: &AppHandle) -> bool {
    let Ok(w) = window(app) else { return false };
    let mut closed = closes(app);
    let mut loads = state(app).loads.subscribe();
    loads.borrow_and_update();
    let _ = w.set_title(SIGN_IN_TITLE);
    show(app);
    let _ = w.navigate(parse(HOME));
    loop {
        tokio::select! {
            _ = closed.changed() => return false,
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

/// Changes each time that the user closes the window.
pub fn closes(app: &AppHandle) -> watch::Receiver<u64> {
    let mut closes = state(app).closes.subscribe();
    closes.borrow_and_update();
    closes
}

/// The cookies of chatgpt.com in the window, with their values (for the capture mode).
pub fn cookies(app: &AppHandle) -> Vec<Cookie<'static>> {
    window(app).ok().and_then(|w| w.cookies_for_url(parse(HOME)).ok()).unwrap_or_default()
}

/// The memory of the web content process of the page, in MB (macOS only).
pub async fn memory_mb(app: &AppHandle) -> Option<u64> {
    crate::platform::web_memory_mb(&window(app).ok()?).await
}

/// Loads the full chatgpt.com app in a second hidden window, and measures its web content process (macOS only).
pub async fn measure_full_page(app: &AppHandle, settle: Duration) -> Option<u64> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let builder = WebviewWindowBuilder::new(app, "chatgpt-measure", WebviewUrl::External(parse("about:blank")))
        .visible(false)
        .on_permission_request(|_, _| PermissionResponse::Deny)
        .on_new_window(|_, _| NewWindowResponse::Deny);
    #[cfg(target_os = "macos")]
    let builder = builder.user_agent(&crate::platform::safari_user_agent()).data_store_identifier(STORE);
    let measured = builder.build().ok()?;
    let _ = measured.navigate(parse(HOME));
    tokio::time::sleep(settle).await;
    let memory = crate::platform::web_memory_mb(&measured).await;
    let _ = measured.destroy();
    memory
}

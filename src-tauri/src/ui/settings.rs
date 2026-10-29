//! The settings window, and the commands of its page.
use crate::{
    account,
    app::{self, State, moulin, set_ready},
    audio, hotkey,
    options::Mode,
    platform,
    settings::{Patch, Settings},
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, webview::PermissionResponse};

pub const LABEL: &str = "settings";

/// What the settings window shows.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    settings: Settings,
    state: State,
    status: String,
    notice: Option<&'static str>,
    signed_in: bool,
    hotkey_error: Option<String>,
    version: String,
    platform: &'static str,
    accessibility: bool,
}

fn snapshot(app: &AppHandle) -> Snapshot {
    let core = moulin(app).core();
    Snapshot {
        settings: core.settings.clone(),
        state: core.state,
        status: core.status.clone(),
        notice: core.notice,
        signed_in: core.signed_in,
        hotkey_error: core.hotkey_error.clone(),
        version: app.package_info().version.to_string(),
        platform: std::env::consts::OS,
        accessibility: platform::accessibility(false),
    }
}

/// Sends the new status to the settings window, if it is open.
pub fn push_status(app: &AppHandle) {
    if app.get_webview_window(LABEL).is_some() {
        let _ = app.emit_to(LABEL, "status", snapshot(app));
    }
}

/// Opens the settings window, or brings it to the front. The test modes have no settings window.
pub fn open(app: &AppHandle) {
    if moulin(app).opts.mode != Mode::App {
        return;
    }
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    if let Err(err) = create(app) {
        eprintln!("Cannot open the settings: {err}");
    }
}

/// Creates the settings window, hidden. Its page gives the window the height of the content, then shows it, so that
/// the window never shows empty or with empty space (see settings.js).
pub fn create(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("settings/settings.html".into()))
        .title("Moulin")
        .inner_size(560.0, 800.0)
        .resizable(false)
        .maximizable(false)
        .visible(false)
        .on_permission_request(|_, _| PermissionResponse::Deny);
    #[cfg(target_os = "macos")]
    let builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
    builder.build()
}

#[tauri::command]
pub fn settings_get(app: AppHandle) -> Snapshot {
    snapshot(&app)
}

#[tauri::command]
pub fn settings_set(app: AppHandle, patch: Patch) -> Snapshot {
    let me = moulin(&app);
    let current = me.core().settings.hotkey.clone();
    if let Some(hotkey) = &patch.hotkey
        && *hotkey != current
    {
        let error = hotkey::change(&app, hotkey).err();
        me.core().hotkey_error = error;
    }
    me.core().settings.apply(patch);
    me.save_settings();
    app::apply_login_item(&app);
    // A good shortcut also clears an old error, for example "This desktop does not give F5 to Moulin".
    let (state, signed_in, hotkey_ok) = {
        let core = me.core();
        (core.state, core.signed_in, core.hotkey_error.is_none())
    };
    if state == State::Idle || (state == State::Error && signed_in && hotkey_ok) {
        set_ready(&app);
    }
    snapshot(&app)
}

/// While the user types a new shortcut, the old one must not start a dictation.
#[tauri::command]
pub fn settings_capture(app: AppHandle, on: bool) {
    hotkey::pause(&app, on);
}

#[tauri::command]
pub async fn account_sign_in(app: AppHandle) -> Snapshot {
    account::sign_in(&app).await;
    snapshot(&app)
}

/// The result is null if the user cancels.
#[tauri::command]
pub async fn account_sign_out(app: AppHandle) -> Option<Snapshot> {
    account::sign_out(&app).await.then(|| snapshot(&app))
}

#[tauri::command]
pub fn open_accessibility() {
    platform::open_accessibility();
}

#[tauri::command]
pub fn microphones() -> Vec<audio::Microphone> {
    audio::microphones()
}

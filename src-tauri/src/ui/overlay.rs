//! The pop-up at the bottom of the screen during a dictation. Its page shows the loudness of the voice, the time,
//! and the result.
use crate::{
    app::{Event, State, moulin, on_main},
    dictation, platform,
};
use serde::Serialize;
use serde_json::json;
use std::time::Duration;
use tauri::{
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    async_runtime::{JoinHandle, spawn},
    utils::config::BackgroundThrottlingPolicy,
    webview::PermissionResponse,
};

pub const LABEL: &str = "overlay";
/// The size of the pop-up, in points.
const WIDTH: f64 = 320.0;
const HEIGHT: f64 = 60.0;
/// The space between the pop-up and the bottom of the screen, in points.
pub const MARGIN: f64 = 28.0;
/// The time between two checks of the mouse pointer while the pop-up records.
const POINTER_CHECK: Duration = Duration::from_millis(50);

/// What the pop-up shows. Its page gets the name in lower case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum View {
    Hidden,
    Recording,
    Transcribing,
    Done,
    Error,
}

impl View {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Recording => "recording",
            Self::Transcribing => "transcribing",
            Self::Done => "done",
            Self::Error => "error",
        }
    }
}

/// Shown or not, the timer that hides the pop-up, and the checks of the pointer.
#[derive(Default)]
pub struct Visibility {
    shown: bool,
    hide_timer: Option<JoinHandle<()>>,
    /// Sends the pointer position to the page while the pop-up records (see `track_pointer`).
    pointer_timer: Option<JoinHandle<()>>,
    /// The last pointer position that the page got.
    pointer: Option<(f64, f64)>,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("overlay/overlay.html".into()))
        .title("Moulin")
        .inner_size(WIDTH, HEIGHT)
        .visible(false)
        .decorations(false)
        .transparent(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .shadow(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        // A click on the pop-up stops the dictation, also when Moulin is not the active app.
        .accept_first_mouse(true)
        .background_throttling(BackgroundThrottlingPolicy::Disabled)
        .on_permission_request(|_, _| PermissionResponse::Deny)
        .build()?;
    platform::setup_overlay(&window);
    Ok(())
}

pub fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

/// Shows a view of the pop-up. With `hide_after`, the pop-up hides after this time.
/// The pop-up stays hidden when the setting "Show the pop-up" is off.
pub fn set(app: &AppHandle, view: View, text: &str, hide_after: Option<Duration>) {
    let (show, was_shown) = {
        let mut core = moulin(app).core();
        let show = view != View::Hidden && core.settings.show_overlay;
        let visibility = &mut core.overlay;
        let was_shown = std::mem::replace(&mut visibility.shown, show);
        if let Some(timer) = visibility.hide_timer.take() {
            timer.abort();
        }
        visibility.hide_timer = hide_after.map(|after| {
            let app = app.clone();
            spawn(async move {
                tokio::time::sleep(after).await;
                hide(&app);
            })
        });
        if show && view == View::Recording {
            if visibility.pointer_timer.is_none() {
                visibility.pointer = None;
                visibility.pointer_timer = Some(spawn(track_pointer(app.clone())));
            }
        } else if let Some(timer) = visibility.pointer_timer.take() {
            timer.abort();
        }
        (show, was_shown)
    };
    let Some(window) = window(app) else { return };
    let _ = app.emit_to(LABEL, "overlay:state", json!({ "state": view, "text": text }));
    if !show {
        let _ = window.hide();
    } else if !was_shown {
        platform::place_overlay(&window, MARGIN);
        platform::show_inactive(&window);
    }
    moulin(app).emit(Event::Overlay { view, text: text.into(), visible: show });
}

pub fn hide(app: &AppHandle) {
    set(app, View::Hidden, "", None);
}

/// The loudness of the voice (from 0 to 1) and the time of the recording, about 14 times per second.
pub fn level(app: &AppHandle, level: f32, ms: u64) {
    let _ = app.emit_to(LABEL, "overlay:level", json!({ "level": level, "ms": ms }));
}

/// The pill turns red under the pointer, because a click on it stops the dictation. WebKit does not report that the
/// pointer leaves a window that is never the key window, so Moulin sends the pointer position to the page itself.
async fn track_pointer(app: AppHandle) {
    loop {
        tokio::time::sleep(POINTER_CHECK).await;
        on_main(&app, send_pointer);
    }
}

/// Sends the pointer position to the page when it changes: points from the top-left corner of the pop-up, or null
/// when the pointer is not over the pop-up.
fn send_pointer(app: &AppHandle) {
    let Some(window) = window(app) else { return };
    let pointer = platform::pointer_in(&window);
    let changed = {
        let mut core = moulin(app).core();
        let visibility = &mut core.overlay;
        // A check that comes after the end of the recording sends nothing.
        visibility.pointer_timer.is_some() && std::mem::replace(&mut visibility.pointer, pointer) != pointer
    };
    if changed {
        let _ = app.emit_to(LABEL, "overlay:pointer", pointer.map(|(x, y)| json!({ "x": x, "y": y })));
    }
}

/// A click on the pop-up stops the dictation.
#[tauri::command]
pub fn overlay_clicked(app: AppHandle) {
    if moulin(&app).core().state == State::Recording {
        spawn(stop_from_click(app));
    }
}

/// macOS: a click on the pop-up makes Moulin the active app, so the paste would go to Moulin.
/// When Moulin hides, macOS gives the front back to the previous app. Then the pop-up shows again without activation.
async fn stop_from_click(app: AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let settings_focused = app.get_webview_window(crate::ui::settings::LABEL).and_then(|w| w.is_focused().ok()).unwrap_or(false);
        if !settings_focused {
            let _ = app.hide();
            tokio::time::sleep(Duration::from_millis(40)).await;
            if moulin(&app).core().state != State::Recording {
                return; // Esc or the time limit stopped the dictation during the wait.
            }
            if let Some(window) = window(&app) {
                platform::show_inactive(&window);
            }
        }
    }
    on_main(&app, dictation::stop);
}

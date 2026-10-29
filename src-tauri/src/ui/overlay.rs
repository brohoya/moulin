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
    AppHandle, Emitter, LogicalPosition, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    async_runtime::{JoinHandle, spawn},
    utils::config::BackgroundThrottlingPolicy,
    webview::PermissionResponse,
};

const LABEL: &str = "overlay";
/// The size of the pop-up, in points.
const WIDTH: f64 = 320.0;
const HEIGHT: f64 = 60.0;
/// The space between the pop-up and the bottom of the screen, in points.
const MARGIN: f64 = 28.0;

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

/// Shown or not, and the timer that hides the pop-up.
#[derive(Default)]
pub struct Visibility {
    shown: bool,
    hide_timer: Option<JoinHandle<()>>,
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
        (show, was_shown)
    };
    let Some(window) = window(app) else { return };
    let _ = app.emit_to(LABEL, "overlay:state", json!({ "state": view, "text": text }));
    if !show {
        let _ = window.hide();
    } else if !was_shown {
        position(app, &window);
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

/// Puts the pop-up at the bottom center of the screen with the mouse pointer.
fn position(app: &AppHandle, window: &WebviewWindow) {
    let Ok(cursor) = app.cursor_position() else { return };
    let monitor = app.monitor_from_point(cursor.x, cursor.y).ok().flatten().or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    // macOS places windows in points, the other systems in pixels.
    if platform::IS_MAC {
        let (x, y) = (f64::from(area.position.x) / scale, f64::from(area.position.y) / scale);
        let (width, height) = (f64::from(area.size.width) / scale, f64::from(area.size.height) / scale);
        let _ = window.set_position(LogicalPosition::new((x + (width - WIDTH) / 2.0).round(), (y + height - HEIGHT - MARGIN).round()));
    } else {
        let (width, height) = ((WIDTH * scale).round() as i32, (HEIGHT * scale).round() as i32);
        let x = area.position.x + (area.size.width as i32 - width) / 2;
        let y = area.position.y + area.size.height as i32 - height - (MARGIN * scale).round() as i32;
        let _ = window.set_position(PhysicalPosition::new(x, y));
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

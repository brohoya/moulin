//! What the user sees: the tray icon and its menu, the pop-up, the settings window, and the notifications.
pub mod overlay;
pub mod settings;
pub mod tray;

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

pub fn notify(app: &AppHandle, body: &str) {
    let _ = app.notification().builder().title("Moulin").body(body).show();
}

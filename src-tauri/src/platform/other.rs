//! Windows and Linux: the permissions need no check, and the pop-up needs no change. The login item is different
//! on each system.
use tauri::{PhysicalPosition, WebviewWindow};

pub fn setup_overlay(_window: &WebviewWindow) {}

pub fn show_inactive(window: &WebviewWindow) {
    let _ = window.show();
}

/// Puts the pop-up at the bottom center of the screen with the mouse pointer, `margin` points above the bottom of the
/// work area (the area without the taskbar).
/// Windows and Linux give the pointer and the screens in pixels.
pub fn place_overlay(window: &WebviewWindow, margin: f64) {
    let Ok(cursor) = window.cursor_position() else { return };
    let monitor = window.monitor_from_point(cursor.x, cursor.y).ok().flatten().or_else(|| window.primary_monitor().ok().flatten());
    let (Some(monitor), Ok(size), Ok(scale)) = (monitor, window.outer_size(), window.scale_factor()) else { return };
    // The size in pixels on the target screen, which can have another scale.
    let size = size.to_logical::<f64>(scale).to_physical::<i32>(monitor.scale_factor());
    let area = monitor.work_area();
    let x = area.position.x + (area.size.width as i32 - size.width) / 2;
    let y = area.position.y + area.size.height as i32 - size.height - (margin * monitor.scale_factor()).round() as i32;
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// The mouse pointer in the window, in logical pixels from the top-left corner of the window, while the pointer is
/// over the window.
pub fn pointer_in(window: &WebviewWindow) -> Option<(f64, f64)> {
    let cursor = window.cursor_position().ok()?;
    let (origin, size, scale) = (window.outer_position().ok()?, window.outer_size().ok()?, window.scale_factor().ok()?);
    let (x, y) = (cursor.x - f64::from(origin.x), cursor.y - f64::from(origin.y));
    let inside = (0.0..=f64::from(size.width)).contains(&x) && (0.0..=f64::from(size.height)).contains(&y);
    inside.then_some((x / scale, y / scale))
}

pub async fn microphone_allowed() -> bool {
    true
}

pub fn accessibility(_prompt: bool) -> bool {
    true
}

pub fn open_accessibility() {}

pub async fn snapshot_png(_window: &WebviewWindow) -> Result<Vec<u8>, String> {
    Err("pictures need macOS".into())
}

pub async fn web_memory_mb(_window: &WebviewWindow) -> Option<u64> {
    None
}

/// The login item on Windows: the Run key of the user. The value name is the app ID, so a new path of the app
/// replaces the old one.
#[cfg(windows)]
pub fn set_login_item(on: bool) -> Result<(), String> {
    let key = windows_registry::CURRENT_USER.create("Software\\Microsoft\\Windows\\CurrentVersion\\Run").map_err(|err| err.to_string())?;
    if !on {
        let _ = key.remove_value("app.moulin");
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    key.set_string("app.moulin", format!("\"{}\"", exe.display())).map_err(|err| err.to_string())
}

/// The login item on Linux: a desktop file in ~/.config/autostart. An AppImage gives its own path in APPIMAGE.
#[cfg(target_os = "linux")]
pub fn set_login_item(on: bool) -> Result<(), String> {
    use std::path::{Path, PathBuf};
    let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
    let file = Path::new(&home).join(".config/autostart/moulin.desktop");
    if !on {
        let _ = std::fs::remove_file(&file);
        return Ok(());
    }
    let exec = std::env::var_os("APPIMAGE").map(PathBuf::from).or_else(|| std::env::current_exe().ok()).ok_or("no executable path")?;
    std::fs::create_dir_all(file.parent().unwrap_or(Path::new("."))).map_err(|err| err.to_string())?;
    let text = format!(
        "[Desktop Entry]\nType=Application\nName=Moulin\nExec=\"{}\"\nIcon=moulin\nX-GNOME-Autostart-enabled=true\n",
        exec.display()
    );
    std::fs::write(&file, text).map_err(|err| err.to_string())
}

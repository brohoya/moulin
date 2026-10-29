//! Windows and Linux: the permissions need no check, and the pop-up needs no change. The login item is different
//! on each system.
use tauri::WebviewWindow;

pub fn setup_overlay(_window: &WebviewWindow) {}

pub fn show_inactive(window: &WebviewWindow) {
    let _ = window.show();
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

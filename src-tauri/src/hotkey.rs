//! The global shortcut of the dictation, and Esc during a recording.
use crate::{
    app::{moulin, on_main},
    dictation, platform,
    settings::HotkeyMode,
};
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

const ESCAPE: &str = "Escape";

/// Registers the shortcut of the dictation. An empty shortcut means no Moulin shortcut: a system shortcut runs
/// "Moulin --toggle". The error is a message for the user.
pub fn register(app: &AppHandle, accelerator: &str) -> Result<(), String> {
    if accelerator.is_empty() {
        return Ok(());
    }
    // The shortcut plugin uses X11. On Wayland, an X11 app gets the keys only while one of its windows has the focus.
    if platform::wayland() {
        return Err(format!(
            "This desktop does not give {accelerator} to Moulin. Add a system shortcut that runs Moulin with --toggle, and remove this one."
        ));
    }
    let shortcut: Shortcut = accelerator.parse().map_err(|_| format!("{accelerator} is not a valid shortcut."))?;
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _, event| {
            let state = event.state;
            on_main(app, move |app| on_key(app, state));
        })
        .map_err(|_| format!("{accelerator} is in use by another app. Choose a different shortcut."))
}

/// True if the shortcut works now.
pub fn is_registered(app: &AppHandle, accelerator: &str) -> bool {
    !accelerator.is_empty() && app.global_shortcut().is_registered(accelerator)
}

/// Changes the shortcut of the dictation. If the new shortcut does not register, the old one stays.
pub fn change(app: &AppHandle, next: &str) -> Result<(), String> {
    let old = moulin(app).core().settings.hotkey.clone();
    if is_registered(app, &old) {
        let _ = app.global_shortcut().unregister(old.as_str());
    }
    if let Err(error) = register(app, next) {
        let _ = register(app, &old);
        return Err(error);
    }
    moulin(app).core().settings.hotkey = next.into();
    Ok(())
}

/// Stops the shortcut while the user types a new one in the settings, so that the old one starts no dictation.
pub fn pause(app: &AppHandle, paused: bool) {
    let hotkey = moulin(app).core().settings.hotkey.clone();
    if hotkey.is_empty() {
        return;
    }
    let registered = is_registered(app, &hotkey);
    if paused && registered {
        let _ = app.global_shortcut().unregister(hotkey.as_str());
    } else if !paused && !registered {
        let _ = register(app, &hotkey);
    }
}

/// Esc cancels a dictation. If another app holds Esc, the tray menu can still cancel.
pub fn set_escape(app: &AppHandle, on: bool) {
    let shortcuts = app.global_shortcut();
    let registered = shortcuts.is_registered(ESCAPE);
    if on && !registered {
        let _ = shortcuts.on_shortcut(ESCAPE, |app, _, event| {
            if event.state == ShortcutState::Pressed {
                on_main(app, dictation::cancel);
            }
        });
    } else if !on && registered {
        let _ = shortcuts.unregister(ESCAPE);
    }
}

/// In toggle mode, a push of the key starts or stops a dictation. In hold mode, the key-down starts the dictation,
/// and the key-up stops it. The shortcut plugin reports the key-up on macOS, Windows, and X11, so the hold mode
/// needs no key hook.
fn on_key(app: &AppHandle, state: ShortcutState) {
    let mode = moulin(app).core().settings.mode;
    match (mode, state) {
        (HotkeyMode::Toggle, ShortcutState::Pressed) => dictation::toggle(app),
        (HotkeyMode::Toggle, ShortcutState::Released) => {}
        (HotkeyMode::Hold, ShortcutState::Pressed) => dictation::press(app),
        (HotkeyMode::Hold, ShortcutState::Released) => dictation::release(app),
    }
}

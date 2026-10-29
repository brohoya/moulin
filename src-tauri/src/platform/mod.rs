//! The parts that differ for each system: the pop-up window, the permissions of macOS, the login item,
//! and the measures of the test modes. Each system has the same functions, so the rest of the app needs no checks
//! of the system.
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(target_os = "macos"))]
mod other;
#[cfg(not(target_os = "macos"))]
pub use other::*;

pub const IS_MAC: bool = cfg!(target_os = "macos");

/// True on a Wayland desktop of Linux. There, the global shortcut does not work, and the paste needs other tools.
pub fn wayland() -> bool {
    cfg!(target_os = "linux") && std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland")
}

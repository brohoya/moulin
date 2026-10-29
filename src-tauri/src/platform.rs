// The parts that differ for each system: the pop-up window, the permissions of macOS, the login item,
// and the measures of the E2E test.
#[cfg(not(target_os = "macos"))]
use tauri::WebviewWindow;

#[cfg(target_os = "macos")]
pub use mac::*;

#[cfg(target_os = "macos")]
mod mac {
    use block2::RcBlock;
    use objc2::{msg_send, rc::Retained, runtime::Bool, sel};
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage, NSScreenSaverWindowLevel, NSWindow, NSWindowCollectionBehavior};
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
    use objc2_foundation::{NSBundle, NSDictionary, NSError, NSNumber, NSString};
    use objc2_service_management::{SMAppService, SMAppServiceStatus};
    use objc2_web_kit::WKWebView;
    use std::{ffi::c_void, sync::Mutex};
    use tauri::WebviewWindow;
    use tokio::sync::oneshot;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    }

    /// The user agent of Safari. The page runs in WebKit, so a Chrome user agent would not match the engine.
    /// The default user agent of WKWebView has no "Safari" part, and some sign-in pages refuse it.
    pub fn safari_user_agent() -> String {
        let version = NSBundle::bundleWithPath(&NSString::from_str("/Applications/Safari.app"))
            .and_then(|bundle| bundle.objectForInfoDictionaryKey(&NSString::from_str("CFBundleShortVersionString")))
            .and_then(|value| value.downcast::<NSString>().ok())
            .map(|value| value.to_string())
            .unwrap_or_else(|| "26.0".into());
        format!(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/{version} Safari/605.1.15"
        )
    }

    fn ns_window(window: &WebviewWindow) -> Option<usize> {
        window.ns_window().ok().map(|pointer| pointer as usize)
    }

    /// The pop-up stays above the windows of full-screen apps and shows on all desktops.
    pub fn setup_overlay(window: &WebviewWindow) {
        let Some(pointer) = ns_window(window) else { return };
        let _ = window.run_on_main_thread(move || {
            // SAFETY: the pointer is the NSWindow of the pop-up, and this code runs on the main thread.
            let ns_window = unsafe { &*(pointer as *const NSWindow) };
            ns_window.setLevel(NSScreenSaverWindowLevel);
            ns_window.setCollectionBehavior(
                ns_window.collectionBehavior()
                    | NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::FullScreenAuxiliary
                    | NSWindowCollectionBehavior::IgnoresCycle,
            );
        });
    }

    /// Shows the pop-up without making Moulin the active app. Tauri's show() makes the window key.
    pub fn show_inactive(window: &WebviewWindow) {
        let Some(pointer) = ns_window(window) else { return };
        let _ = window.run_on_main_thread(move || {
            // SAFETY: as above.
            unsafe { &*(pointer as *const NSWindow) }.orderFrontRegardless();
        });
    }

    pub enum Access {
        Granted,
        Undetermined,
        Denied,
    }

    pub fn microphone_access() -> Access {
        let Some(audio) = (unsafe { AVMediaTypeAudio }) else { return Access::Denied };
        match unsafe { AVCaptureDevice::authorizationStatusForMediaType(audio) } {
            AVAuthorizationStatus::Authorized => Access::Granted,
            AVAuthorizationStatus::NotDetermined => Access::Undetermined,
            _ => Access::Denied,
        }
    }

    /// Shows the prompt of macOS for the microphone. Resolves true if the user allows it.
    pub async fn request_microphone() -> bool {
        let Some(audio) = (unsafe { AVMediaTypeAudio }) else { return false };
        let (tx, rx) = oneshot::channel();
        {
            // AVFoundation copies the block, so this reference can go before the wait.
            let tx = Mutex::new(Some(tx));
            let handler = RcBlock::new(move |granted: Bool| {
                if let Some(tx) = tx.lock().unwrap().take() {
                    let _ = tx.send(granted.as_bool());
                }
            });
            unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(audio, &handler) };
        }
        rx.await.unwrap_or(false)
    }

    /// True if Moulin can send key events. With `prompt`, macOS shows its prompt for Accessibility if needed.
    pub fn accessibility(prompt: bool) -> bool {
        let options = NSDictionary::from_slices(&[&*NSString::from_str("AXTrustedCheckOptionPrompt")], &[&*NSNumber::new_bool(prompt)]);
        unsafe { AXIsProcessTrustedWithOptions(Retained::as_ptr(&options).cast()) }
    }

    pub fn open_privacy_pane(pane: &str) {
        let url = format!("x-apple.systempreferences:com.apple.preference.security?{pane}");
        let _ = std::process::Command::new("open").arg(url).spawn();
    }

    /// The login item of the app (SMAppService).
    pub fn set_login_item(on: bool) -> Result<(), String> {
        let service = unsafe { SMAppService::mainAppService() };
        let enabled = unsafe { service.status() } == SMAppServiceStatus::Enabled;
        let result = match (on, enabled) {
            (true, false) => unsafe { service.registerAndReturnError() },
            (false, true) => unsafe { service.unregisterAndReturnError() },
            _ => Ok(()),
        };
        result.map_err(|err: Retained<NSError>| err.localizedDescription().to_string())
    }

    /// A PNG picture of the page of a window (the E2E test takes one of the pop-up in each state).
    pub async fn snapshot_png(window: &WebviewWindow) -> Result<Vec<u8>, String> {
        let (tx, rx) = oneshot::channel();
        window
            .with_webview(move |webview| {
                let tx = Mutex::new(Some(tx));
                let handler = RcBlock::new(move |image: *mut NSImage, _error: *mut NSError| {
                    // SAFETY: WebKit gives a valid image or null.
                    let png = unsafe { image.as_ref() }.and_then(|image| {
                        let tiff = image.TIFFRepresentation()?;
                        let bitmap = NSBitmapImageRep::imageRepWithData(&tiff)?;
                        unsafe { bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }
                    });
                    if let Some(tx) = tx.lock().unwrap().take() {
                        let _ = tx.send(png.map(|data| data.to_vec()));
                    }
                });
                // SAFETY: the pointer is the WKWebView of the window, and this closure runs on the main thread.
                let view = unsafe { &*(webview.inner() as *const WKWebView) };
                unsafe { view.takeSnapshotWithConfiguration_completionHandler(None, &handler) };
            })
            .map_err(|err| err.to_string())?;
        rx.await.ok().flatten().ok_or_else(|| "WebKit gave no picture".into())
    }

    /// The resident memory of the web content process of a window, in MB.
    /// WebKit gives the process ID only through a private method, so the code checks that the method exists.
    pub async fn web_memory_mb(window: &WebviewWindow) -> Option<u64> {
        let (tx, rx) = oneshot::channel();
        window
            .with_webview(move |webview| {
                // SAFETY: as above.
                let view = unsafe { &*(webview.inner() as *const WKWebView) };
                let known: bool = unsafe { msg_send![view, respondsToSelector: sel!(_webProcessIdentifier)] };
                let pid: i32 = if known { unsafe { msg_send![view, _webProcessIdentifier] } } else { 0 };
                let _ = tx.send(pid);
            })
            .ok()?;
        let pid = rx.await.ok().filter(|pid| *pid > 0)?;
        let output = std::process::Command::new("ps").args(["-o", "rss=", "-p", &pid.to_string()]).output().ok()?;
        let kb: u64 = String::from_utf8_lossy(&output.stdout).trim().parse().ok()?;
        Some(kb / 1024)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn setup_overlay(_window: &WebviewWindow) {}

#[cfg(not(target_os = "macos"))]
pub fn show_inactive(window: &WebviewWindow) {
    let _ = window.show();
}

#[cfg(not(target_os = "macos"))]
pub async fn snapshot_png(_window: &WebviewWindow) -> Result<Vec<u8>, String> {
    Err("pictures need macOS".into())
}

#[cfg(not(target_os = "macos"))]
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
    let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
    let file = std::path::Path::new(&home).join(".config/autostart/moulin.desktop");
    if !on {
        let _ = std::fs::remove_file(&file);
        return Ok(());
    }
    let exec =
        std::env::var_os("APPIMAGE").map(std::path::PathBuf::from).or_else(|| std::env::current_exe().ok()).ok_or("no executable path")?;
    std::fs::create_dir_all(file.parent().unwrap_or(std::path::Path::new("."))).map_err(|err| err.to_string())?;
    let text = format!(
        "[Desktop Entry]\nType=Application\nName=Moulin\nExec=\"{}\"\nIcon=moulin\nX-GNOME-Autostart-enabled=true\n",
        exec.display()
    );
    std::fs::write(&file, text).map_err(|err| err.to_string())
}

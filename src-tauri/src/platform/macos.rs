//! macOS: the level of the pop-up, the permissions (microphone and Accessibility), the login item (SMAppService),
//! and the WebKit measures of the test modes.
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
    format!("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/{version} Safari/605.1.15")
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

/// True if Moulin can record the microphone. macOS asks one time. A refusal needs a change in System Settings,
/// so this function then opens the pane of the microphone.
pub async fn microphone_allowed() -> bool {
    // SAFETY: AVMediaTypeAudio is a constant of AVFoundation.
    let status = unsafe { AVMediaTypeAudio }.map(|audio| unsafe { AVCaptureDevice::authorizationStatusForMediaType(audio) });
    match status {
        Some(AVAuthorizationStatus::Authorized) => true,
        Some(AVAuthorizationStatus::NotDetermined) => request_microphone().await,
        _ => {
            open_privacy_pane("Privacy_Microphone");
            false
        }
    }
}

/// Shows the prompt of macOS for the microphone. Resolves true if the user allows it.
async fn request_microphone() -> bool {
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

/// Asks for the Accessibility permission. Without it, also opens its pane in System Settings.
pub fn open_accessibility() {
    if !accessibility(true) {
        open_privacy_pane("Privacy_Accessibility");
    }
}

fn open_privacy_pane(pane: &str) {
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

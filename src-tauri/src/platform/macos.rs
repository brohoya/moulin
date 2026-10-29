//! macOS: the level of the pop-up, the permissions (microphone and Accessibility), the login item (SMAppService),
//! and the WebKit measures of the test modes.
use block2::RcBlock;
use objc2::{MainThreadMarker, msg_send, rc::Retained, runtime::Bool, sel};
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSEvent, NSImage, NSScreen, NSScreenSaverWindowLevel, NSWindow, NSWindowCollectionBehavior,
    NSWorkspace,
};
use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
use objc2_foundation::{NSArray, NSBundle, NSDictionary, NSError, NSNumber, NSPoint, NSRect, NSString};
use objc2_service_management::{SMAppService, SMAppServiceStatus};
use objc2_web_kit::WKWebView;
use std::{ffi::c_void, sync::Mutex};
use tauri::WebviewWindow;
use tokio::sync::oneshot;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> *mut NSArray<NSDictionary>;
    fn CGRectMakeWithDictionaryRepresentation(dict: *const NSDictionary, rect: *mut NSRect) -> bool;
}

/// kCGWindowListOptionOnScreenOnly and kCGWindowListExcludeDesktopElements: the visible windows, front to back.
const VISIBLE_WINDOWS: u32 = (1 << 0) | (1 << 4);

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

/// Puts the pop-up at the bottom center of the screen that the user works on, `margin` points above the bottom of the
/// visible frame (the area without the menu bar and the Dock).
/// AppKit places all windows in points from the bottom-left corner of the main screen, so no scale factor applies.
pub fn place_overlay(window: &WebviewWindow, margin: f64) {
    let Some(pointer) = ns_window(window) else { return };
    let _ = window.run_on_main_thread(move || {
        let Some(screen) = MainThreadMarker::new().and_then(active_screen) else { return };
        // SAFETY: as above.
        let ns_window = unsafe { &*(pointer as *const NSWindow) };
        let (area, size) = (screen.visibleFrame(), ns_window.frame().size);
        ns_window
            .setFrameOrigin(NSPoint::new((area.origin.x + (area.size.width - size.width) / 2.0).round(), (area.origin.y + margin).round()));
    });
}

/// The screen that the user works on: the screen with the largest part of the front window of the active app.
/// Without such a window (for example only the desktop), the screen with the mouse pointer.
fn active_screen(mtm: MainThreadMarker) -> Option<Retained<NSScreen>> {
    let screens = NSScreen::screens(mtm).to_vec();
    // The main screen is the first one. The window list measures from its top-left corner.
    let main_height = screens.first()?.frame().size.height;
    let by_window = front_window(main_height).and_then(|window| {
        let largest = screens.iter().max_by(|a, b| overlap(a.frame(), window).total_cmp(&overlap(b.frame(), window)))?;
        (overlap(largest.frame(), window) > 0.0).then(|| largest.clone())
    });
    let mouse = NSEvent::mouseLocation();
    by_window.or_else(|| screens.iter().find(|screen| contains(screen.frame(), mouse)).cloned()).or_else(|| screens.first().cloned())
}

/// The frame of the front window of the active app, in AppKit coordinates. Only the normal windows count (layer 0):
/// not the menu bar, the Dock, or the pop-up. The window list gives the frames without a permission.
fn front_window(main_height: f64) -> Option<NSRect> {
    let pid = NSWorkspace::sharedWorkspace().frontmostApplication()?.processIdentifier();
    // SAFETY: the function gives a new array or null, and the caller owns the array.
    let windows = unsafe { Retained::from_raw(CGWindowListCopyWindowInfo(VISIBLE_WINDOWS, 0)) }?;
    let number =
        |info: &NSDictionary, key: &str| info.objectForKey(&NSString::from_str(key))?.downcast::<NSNumber>().ok().map(|n| n.intValue());
    let frame = windows.to_vec().into_iter().find_map(|info| {
        if number(&info, "kCGWindowOwnerPID")? != pid || number(&info, "kCGWindowLayer")? != 0 {
            return None;
        }
        let bounds = info.objectForKey(&NSString::from_str("kCGWindowBounds"))?;
        let mut frame = NSRect::ZERO;
        // SAFETY: the bounds are a dictionary of the window list, and `frame` is a valid output.
        unsafe { CGRectMakeWithDictionaryRepresentation(Retained::as_ptr(&bounds).cast(), &mut frame) }.then_some(frame)
    })?;
    // The window list counts y down from the top of the main screen. AppKit counts y up from its bottom.
    Some(NSRect::new(NSPoint::new(frame.origin.x, main_height - frame.origin.y - frame.size.height), frame.size))
}

/// The area of the part of `a` that is also in `b`.
fn overlap(a: NSRect, b: NSRect) -> f64 {
    let width = (a.origin.x + a.size.width).min(b.origin.x + b.size.width) - a.origin.x.max(b.origin.x);
    let height = (a.origin.y + a.size.height).min(b.origin.y + b.size.height) - a.origin.y.max(b.origin.y);
    width.max(0.0) * height.max(0.0)
}

fn contains(rect: NSRect, point: NSPoint) -> bool {
    (rect.origin.x..=rect.origin.x + rect.size.width).contains(&point.x)
        && (rect.origin.y..=rect.origin.y + rect.size.height).contains(&point.y)
}

/// The mouse pointer in the window, in points from the top-left corner of the window, while the pointer is over
/// the window. Only the main thread gets a value.
pub fn pointer_in(window: &WebviewWindow) -> Option<(f64, f64)> {
    MainThreadMarker::new()?;
    let pointer = ns_window(window)?;
    // SAFETY: the pointer is the NSWindow of `window`, and this code runs on the main thread.
    let frame = unsafe { &*(pointer as *const NSWindow) }.frame();
    let mouse = NSEvent::mouseLocation();
    contains(frame, mouse).then_some((mouse.x - frame.origin.x, frame.origin.y + frame.size.height - mouse.y))
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

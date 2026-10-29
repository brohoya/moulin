//! The Cloudflare check. Cloudflare protects chatgpt.com from bots. Sometimes it refuses a request (HTTP 403 with the
//! header cf-mitigated) until a chatgpt.com page passes a check: a short Cloudflare script, and sometimes a click on a
//! box. When the check passes, Cloudflare gives the browser a pass: the cookie cf_clearance.
//!
//! The light page is plain text, so no script runs in it. After a refusal, the full page runs the check, first in the
//! hidden window, then in the window that the user sees. After each step, the caller sends its request again: only the
//! answer of Cloudflare tells if the check passed.
use super::{HOME, closes, load, parse, show, window};
use crate::platform;
use std::time::{Duration, Instant};
use tauri::AppHandle;

/// The cookie of the pass. Each check that passes gives it a new value. Moulin only compares the values.
const PASS: &str = "cf_clearance";
/// Most checks pass in a few seconds, without a click.
const HIDDEN_WAIT: Duration = Duration::from_secs(10);
/// The time for the user to complete the check.
const SHOWN_WAIT: Duration = Duration::from_secs(120);
/// The title of the window during the check.
const TITLE: &str = "Complete the Cloudflare check - Moulin";

/// The status during the check in the hidden window. The pop-up shows it too.
pub const WAITING: &str = "Waiting for the Cloudflare check";
/// The status while the window shows the check.
pub const ASKING: &str = "Complete the Cloudflare check";
/// The explanation in the settings window during the check.
pub const CHECK_NOTICE: &str = "Cloudflare, the bot protection of chatgpt.com, checks that a person uses Moulin. \
    Usually the check passes by itself. If Moulin shows the ChatGPT window, complete the check in it. \
    Moulin then continues by itself.";
/// The error after the last refusal of an upload.
pub const UPLOAD_REFUSED: &str = "Cloudflare refused the upload";
pub const UPLOAD_NOTICE: &str = "Cloudflare, the bot protection of chatgpt.com, refused the upload. \
    Your ChatGPT sign-in is not the cause. Moulin keeps the recording. Wait a few minutes. \
    Then click \"Send the last recording again\" in the Moulin menu.";
/// The error after the last refusal of the page, at the start or before a dictation.
pub const PAGE_REFUSED: &str = "Cloudflare refused the request";
pub const PAGE_NOTICE: &str = "Cloudflare, the bot protection of chatgpt.com, refused the request from Moulin. \
    Your ChatGPT sign-in is not the cause. Wait a few minutes. Then start a dictation again.";

/// The steps of the check.
#[derive(Clone, Copy, Debug)]
pub enum Step {
    /// The full page runs the check in the hidden window.
    Hidden,
    /// The window shows the check, and the user completes it.
    Shown,
}

impl Step {
    /// The steps in their order.
    pub const ALL: [Self; 2] = [Self::Hidden, Self::Shown];
}

/// Runs one step of the check. The step ends when Cloudflare gives a new pass, when the user closes the window, or at
/// its time limit.
pub async fn run(app: &AppHandle, step: Step) {
    let old = pass(app);
    match step {
        // Cloudflare shows its check when a page loads. So Moulin loads the full page again, also when it is open.
        Step::Hidden => {
            load(app, HOME).await;
            wait(app, &old, HIDDEN_WAIT).await;
        }
        Step::Shown => {
            let Ok(w) = window(app) else { return };
            let _ = w.set_title(TITLE);
            show(app);
            wait(app, &old, SHOWN_WAIT).await;
            let _ = w.hide();
            // Without a window, Moulin can stay the active app, and a paste would go to Moulin.
            platform::return_focus(app);
        }
    }
}

/// The value of the pass, or None. It stays in Moulin: no log and no report shows it.
fn pass(app: &AppHandle) -> Option<String> {
    let cookies = window(app).ok()?.cookies_for_url(parse(HOME)).ok()?;
    cookies.into_iter().find(|cookie| cookie.name() == PASS).map(|cookie| cookie.value().to_string())
}

/// Waits until the pass changes, the user closes the window, or the time ends.
async fn wait(app: &AppHandle, old: &Option<String>, limit: Duration) {
    let mut closed = closes(app);
    let end = Instant::now() + limit;
    while Instant::now() < end && pass(app) == *old {
        tokio::select! {
            _ = closed.changed() => return,
            _ = tokio::time::sleep(Duration::from_millis(500)) => {}
        }
    }
}

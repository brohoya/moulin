//! The ChatGPT account of Moulin: the check of the session, the sign-in, and the sign-out.
use crate::{
    app::{self, State, moulin, set_ready, set_state},
    chatgpt::{self, check::Step},
    ui::{self, overlay},
};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tokio::sync::oneshot;

pub const NOT_SIGNED_IN: &str = "Not signed in to ChatGPT";

/// Opens the light page, and keeps the result: the session has a token or not. When Cloudflare refuses the page, the
/// full page runs the check (see chatgpt/check.rs), and Moulin opens the light page again after each step. After the
/// check, the state and its text come back.
pub async fn check(app: &AppHandle) -> chatgpt::Page {
    let mut page = chatgpt::start(app).await;
    if page.status == 403 {
        let (state, status) = {
            let core = moulin(app).core();
            (core.state, core.status.clone())
        };
        for step in Step::ALL {
            app::show_check(app, step);
            chatgpt::check::run(app, step).await;
            page = chatgpt::start(app).await;
            if page.status != 403 {
                break;
            }
        }
        overlay::hide(app);
        set_state(app, state, status);
    }
    moulin(app).core().signed_in = page.signed_in;
    page
}

/// Shows chatgpt.com, and waits until the user signs in or closes the window.
pub async fn sign_in(app: &AppHandle) {
    let state = moulin(app).core().state;
    set_state(app, state, "Sign in to ChatGPT in the window");
    let ok = chatgpt::sign_in(app).await;
    moulin(app).core().signed_in = ok;
    if ok {
        set_ready(app);
    } else {
        set_state(app, State::Error, NOT_SIGNED_IN);
    }
}

/// Asks for a confirmation, then deletes the chatgpt.com session of Moulin. Returns false if the user cancels.
/// WebKit shows no window.confirm() dialog, so Moulin asks with a dialog of the system.
pub async fn sign_out(app: &AppHandle) -> bool {
    let (tx, rx) = oneshot::channel();
    let mut dialog = app
        .dialog()
        .message("Sign out of ChatGPT in Moulin? Moulin deletes its cookies for chatgpt.com.")
        .title("Moulin")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom("Sign out".into(), "Cancel".into()));
    if let Some(window) = app.get_webview_window(ui::settings::LABEL) {
        dialog = dialog.parent(&window);
    }
    dialog.show(move |ok| {
        let _ = tx.send(ok);
    });
    if !rx.await.unwrap_or(false) {
        return false;
    }
    let page = chatgpt::sign_out(app).await;
    moulin(app).core().signed_in = page.signed_in;
    set_state(app, State::Error, NOT_SIGNED_IN);
    true
}

//! The icon in the menu bar (macOS) or in the system tray (Windows, Linux), and its menu.
//! The icon shows the state. While Moulin transcribes, the sails of the icon turn.
use crate::{
    account,
    app::{State, moulin, on_main},
    dictation, hotkey,
    platform::IS_MAC,
    ui,
};
use std::{sync::LazyLock, time::Duration};
use tauri::{
    AppHandle, Wry,
    async_runtime::{JoinHandle, spawn},
    image::Image,
    menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
};
use tauri_plugin_clipboard_manager::ClipboardExt;

const ID: &str = "main";
/// The number of images of the turning sails.
const FRAMES: usize = 4;
/// The time of each image of the turning sails.
const FRAME_TIME: Duration = Duration::from_millis(120);
/// The menu shows the start of each recent text, with this number of characters.
const SHORT: usize = 50;

struct Images {
    idle: Image<'static>,
    busy: [Image<'static>; FRAMES],
    recording: Image<'static>,
    error: Image<'static>,
}

// Black template images on macOS (the menu bar colors them), colored tiles on Windows and Linux.
// The images have two times the size of the tray, so they are sharp on a Retina screen. The system scales them.
#[cfg(target_os = "macos")]
macro_rules! tray_png {
    ($name:literal) => {
        Image::from_bytes(include_bytes!(concat!("../../icons/tray/mac-", $name, ".png"))).expect("a valid tray image")
    };
}
#[cfg(not(target_os = "macos"))]
macro_rules! tray_png {
    ($name:literal) => {
        Image::from_bytes(include_bytes!(concat!("../../icons/tray/tile-", $name, ".png"))).expect("a valid tray image")
    };
}

static IMAGES: LazyLock<Images> = LazyLock::new(|| Images {
    idle: tray_png!("idle"),
    busy: [tray_png!("busy0"), tray_png!("busy1"), tray_png!("busy2"), tray_png!("busy3")],
    recording: tray_png!("recording"),
    error: tray_png!("error"),
});

/// The turning sails: the current image, and the timer that changes it.
#[derive(Default)]
pub struct Spinner {
    frame: usize,
    timer: Option<JoinHandle<()>>,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    TrayIconBuilder::with_id(ID)
        .icon(IMAGES.idle.clone())
        .icon_as_template(IS_MAC)
        .tooltip("Moulin: Starting")
        .menu(&menu(app)?)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .build(app)?;
    Ok(())
}

/// Shows the current state: the icon, the tooltip, and the menu. All tray changes run on the main thread, and each
/// one reads the current state, so the last change always wins.
pub fn update(app: &AppHandle) {
    {
        let mut core = moulin(app).core();
        let transcribing = core.state == State::Transcribing;
        let spinner = &mut core.spinner;
        spinner.frame = 0;
        if transcribing {
            spinner.timer.get_or_insert_with(|| spawn(spin(app.clone())));
        } else if let Some(timer) = spinner.timer.take() {
            timer.abort();
        }
    }
    on_main(app, |app| {
        render_icon(app);
        let Some(tray) = app.tray_by_id(ID) else { return };
        let status = moulin(app).core().status.clone();
        let _ = tray.set_tooltip(Some(format!("Moulin: {status}")));
        if let Ok(menu) = menu(app) {
            let _ = tray.set_menu(Some(menu));
        }
    });
}

async fn spin(app: AppHandle) {
    loop {
        tokio::time::sleep(FRAME_TIME).await;
        {
            let mut core = moulin(&app).core();
            if core.state != State::Transcribing {
                core.spinner.timer = None;
                return;
            }
            core.spinner.frame = (core.spinner.frame + 1) % FRAMES;
        }
        on_main(&app, render_icon);
    }
}

fn render_icon(app: &AppHandle) {
    let (state, frame) = {
        let core = moulin(app).core();
        (core.state, core.spinner.frame)
    };
    let (image, template) = match state {
        State::Transcribing => (&IMAGES.busy[frame], true),
        State::Recording => (&IMAGES.recording, false),
        State::Error => (&IMAGES.error, false),
        State::Idle => (&IMAGES.idle, true),
    };
    if let Some(tray) = app.tray_by_id(ID) {
        let _ = tray.set_icon_with_as_template(Some(image.clone()), IS_MAC && template);
    }
}

fn menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let (state, status, pending, recent, signed_in, hotkey) = {
        let core = moulin(app).core();
        (core.state, core.status.clone(), core.dictation.has_pending(), core.recent.clone(), core.signed_in, core.settings.hotkey.clone())
    };
    let busy = state.busy();
    let accelerator = hotkey::is_registered(app, &hotkey).then_some(hotkey.as_str());
    let item = |id: &str, text: &str, enabled: bool, accelerator: Option<&str>| {
        // The menu only shows the shortcut. A shortcut that the menu cannot show does not stop the menu.
        MenuItem::with_id(app, id, text, enabled, accelerator).or_else(|_| MenuItem::with_id(app, id, text, enabled, None::<&str>))
    };
    let toggle_text = if state == State::Recording { "Stop dictation" } else { "Start dictation" };
    let mut items: Vec<Box<dyn IsMenuItem<Wry>>> = vec![
        Box::new(item("status", &status, false, None)?),
        Box::new(PredefinedMenuItem::separator(app)?),
        Box::new(item("toggle", toggle_text, state != State::Transcribing, accelerator)?),
    ];
    if state == State::Recording {
        items.push(Box::new(item("cancel", "Cancel dictation", true, Some("Escape"))?));
    }
    if pending && !busy {
        items.push(Box::new(item("resend", "Send the last recording again", true, None)?));
    }
    let texts: Vec<MenuItem<Wry>> = if recent.is_empty() {
        vec![item("nothing", "Nothing yet", false, None)?]
    } else {
        recent.iter().enumerate().map(|(i, text)| item(&format!("recent-{i}"), &shorten(text), true, None)).collect::<tauri::Result<_>>()?
    };
    let refs: Vec<&dyn IsMenuItem<Wry>> = texts.iter().map(|t| t as &dyn IsMenuItem<Wry>).collect();
    items.push(Box::new(Submenu::with_items(app, "Recent dictations", true, &refs)?));
    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    if !signed_in {
        items.push(Box::new(item("sign-in", "Sign in to ChatGPT…", !busy, None)?));
    }
    items.push(Box::new(item("settings", "Settings…", true, None)?));
    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(item("quit", "Quit Moulin", true, None)?));
    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|i| i.as_ref()).collect();
    Menu::with_items(app, &refs)
}

fn shorten(text: &str) -> String {
    if text.chars().count() > SHORT { format!("{}…", text.chars().take(SHORT).collect::<String>().trim()) } else { text.to_string() }
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "toggle" => dictation::toggle(app),
        "cancel" => dictation::cancel(app),
        "resend" => dictation::resend(app),
        "sign-in" => {
            let app = app.clone();
            spawn(async move { account::sign_in(&app).await });
        }
        "settings" => ui::settings::open(app),
        "quit" => app.exit(0),
        _ => copy_recent(app, id),
    }
}

/// A click on a recent text puts it in the clipboard again.
fn copy_recent(app: &AppHandle, id: &str) {
    let index = id.strip_prefix("recent-").and_then(|i| i.parse::<usize>().ok());
    let text = index.and_then(|i| moulin(app).core().recent.get(i).cloned());
    if let Some(text) = text {
        let _ = app.clipboard().write_text(text);
    }
}

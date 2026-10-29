//! Moulin: push the shortcut, speak, push it again. The text goes to the clipboard, and to the cursor if "paste" is on.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod account;
mod app;
mod audio;
mod capture;
mod chatgpt;
mod dictation;
mod e2e;
mod hotkey;
mod options;
mod paste;
mod platform;
mod report;
mod settings;
mod ui;

use app::{Moulin, State, moulin, on_main, set_ready, set_state};
use options::{Mode, Options};
use settings::HotkeyMode;
use tauri::{App, AppHandle, Manager, RunEvent, async_runtime::spawn};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

fn main() {
    let opts = Options::parse();
    tauri::Builder::default()
        // A second start sends its request to the running Moulin: --toggle starts or stops a dictation, a plain start
        // opens the settings.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if argv.iter().any(|a| a == "--toggle") {
                on_main(app, dictation::toggle);
            } else {
                on_main(app, ui::settings::open);
            }
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(chatgpt::ChatGpt::new())
        // The capabilities give each window only its own commands (see build.rs).
        .invoke_handler(tauri::generate_handler![
            ui::overlay::overlay_clicked,
            ui::settings::settings_get,
            ui::settings::settings_set,
            ui::settings::settings_capture,
            ui::settings::account_sign_in,
            ui::settings::account_sign_out,
            ui::settings::open_accessibility,
            ui::settings::microphones,
        ])
        .setup(|app| setup(app, opts))
        .build(tauri::generate_context!())
        .expect("Moulin cannot start")
        .run(on_event);
}

/// Creates the state, the tray, and the windows. Then the start-up continues in a task.
fn setup(app: &mut App, opts: Options) -> Result<(), Box<dyn std::error::Error>> {
    let settings_file = app.path().app_config_dir()?.join("settings.json");
    let first_run = !settings_file.exists();
    let mut settings = settings::load(&settings_file);
    if opts.mode == Mode::E2e && opts.hold {
        settings.mode = HotkeyMode::Hold; // The test only: Moulin saves no settings in E2E mode.
    }
    let mode = opts.mode;
    app.manage(Moulin::new(opts, settings_file, settings));
    let handle = app.handle().clone();
    if mode == Mode::Capture {
        chatgpt::create(&handle, true, Some(capture::SCRIPT))?;
    } else {
        // macOS: no Dock icon, only the icon in the menu bar.
        #[cfg(target_os = "macos")]
        app.set_activation_policy(tauri::ActivationPolicy::Accessory);
        ui::tray::create(&handle)?;
        ui::overlay::create(&handle)?;
        chatgpt::create(&handle, false, None)?;
    }
    spawn(startup(handle, first_run));
    Ok(())
}

/// The start-up: the shortcut, the login item, the chatgpt.com page, and the first state.
async fn startup(app: AppHandle, first_run: bool) {
    let me = moulin(&app);
    if me.opts.mode == Mode::Capture {
        capture::run(app.clone()).await;
        return;
    }
    set_state(&app, State::Idle, "Starting");
    let hotkey_error = hotkey::register(&app, &me.settings().hotkey).err();
    me.core().hotkey_error = hotkey_error.clone();
    app::apply_login_item(&app);
    let page = account::check(&app).await;
    if page.status == 403 {
        set_state(&app, State::Error, chatgpt::CHALLENGE);
        chatgpt::show(&app);
    } else if !page.signed_in {
        set_state(&app, State::Error, account::NOT_SIGNED_IN);
        ui::settings::open(&app);
    } else if let Some(error) = hotkey_error {
        set_state(&app, State::Error, error);
    } else {
        set_ready(&app);
    }
    match me.opts.mode {
        Mode::App => {
            if first_run {
                me.save_settings();
                ui::settings::open(&app);
                ask_permissions_once(&app).await;
            }
            if me.opts.toggle {
                on_main(&app, dictation::toggle); // A system shortcut started Moulin: start the dictation now.
            }
        }
        Mode::E2e => e2e::run(app.clone(), page).await,
        Mode::Capture => {}
    }
}

/// The first start: ask for the microphone and for Accessibility now, so that the first dictation records and pastes.
/// Only macOS asks.
async fn ask_permissions_once(app: &AppHandle) {
    platform::microphone_allowed().await;
    if moulin(app).core().settings.auto_paste {
        platform::accessibility(true);
    }
}

fn on_event(app: &AppHandle, event: RunEvent) {
    match event {
        // Moulin stays in the tray when the last window closes.
        RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
        RunEvent::Exit => {
            chatgpt::quit(app);
            let _ = app.global_shortcut().unregister_all();
        }
        // A click on Moulin in the Finder or the Dock while it runs: open the settings.
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => ui::settings::open(app),
        _ => {}
    }
}

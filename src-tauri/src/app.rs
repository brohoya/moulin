//! The shared state of Moulin, and the status that the tray, the pop-up, and the settings window show.
use crate::{
    audio, chatgpt, dictation,
    options::{Mode, Options},
    platform,
    settings::{self, HotkeyMode, Settings},
    ui::{
        self,
        overlay::{self, View},
        tray,
    },
};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};
use tokio::sync::broadcast;

/// The state of the app. The tray icon, the menu, and the settings window show it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    #[default]
    Idle,
    Recording,
    Transcribing,
    Error,
}

impl State {
    /// True during a dictation: the recording and the upload.
    pub fn busy(self) -> bool {
        matches!(self, Self::Recording | Self::Transcribing)
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Recording => "recording",
            Self::Transcribing => "transcribing",
            Self::Error => "error",
        }
    }
}

/// The events of a dictation. The E2E test listens to them.
#[derive(Clone)]
pub enum Event {
    State(State),
    Overlay {
        view: View,
        text: String,
        visible: bool,
    },
    /// The microphone is open.
    Started,
    MicError(String),
    /// The audio of the dictation, before the upload.
    Audio(Arc<audio::Recording>),
    /// The end of the upload.
    Transcription(Transcription),
}

/// The end of the upload of a dictation.
#[derive(Clone)]
pub struct Transcription {
    pub outcome: chatgpt::Outcome,
    /// The time when the text went to the clipboard, in ms since 1970.
    pub clipboard_at: Option<u64>,
    /// The time of the stop, in ms since 1970.
    pub stop_at: u64,
}

/// The state that Tauri keeps for the app.
pub struct Moulin {
    core: Mutex<Core>,
    pub opts: Options,
    pub events: broadcast::Sender<Event>,
    settings_file: PathBuf,
}

/// The state that changes. One lock protects all of it, so that each change sees a complete state.
#[derive(Default)]
pub struct Core {
    pub settings: Settings,
    pub state: State,
    /// The text of the state: the first item of the menu, the tooltip of the tray, and the settings window.
    pub status: String,
    pub signed_in: bool,
    /// The reason why the shortcut does not work, for the settings window.
    pub hotkey_error: Option<String>,
    /// The last texts of this session, for the tray menu. The newest text is first.
    pub recent: Vec<String>,
    pub dictation: dictation::Session,
    pub overlay: overlay::Visibility,
    pub spinner: tray::Spinner,
}

impl Moulin {
    pub fn new(opts: Options, settings_file: PathBuf, settings: Settings) -> Self {
        Self {
            core: Mutex::new(Core { settings, status: "Starting".into(), ..Core::default() }),
            opts,
            events: broadcast::channel(256).0,
            settings_file,
        }
    }

    /// Locks the state. A panic in another thread does not make the state unusable.
    pub fn core(&self) -> MutexGuard<'_, Core> {
        self.core.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn settings(&self) -> Settings {
        self.core().settings.clone()
    }

    pub fn hotkey_error(&self) -> Option<String> {
        self.core().hotkey_error.clone()
    }

    pub fn emit(&self, event: Event) {
        let _ = self.events.send(event);
    }

    pub fn save_settings(&self) {
        let settings = self.settings();
        if let Err(err) = settings::save(&self.settings_file, &settings) {
            eprintln!("Cannot write {}: {err}", self.settings_file.display());
        }
    }
}

pub fn moulin(app: &AppHandle) -> &Moulin {
    app.state::<Moulin>().inner()
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Runs a step of the dictation on the main thread. The steps never overlap, so a stop cannot come between
/// the parts of a start. The shortcut handlers also return at once: on X11, the shortcut plugin calls them on its
/// own thread, and an unregister from that thread waits for itself.
///
/// Caution: when the caller is on the main thread, the step runs at once. Do not call this function while you
/// hold the lock of `Moulin::core`.
pub fn on_main(app: &AppHandle, step: impl FnOnce(&AppHandle) + Send + 'static) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || step(&handle));
}

/// Changes the state and its text. The tray, the settings window, and the E2E test follow the change.
pub fn set_state(app: &AppHandle, next: State, text: impl Into<String>) {
    let text = text.into();
    println!("[{}] {text}", next.as_str());
    {
        let mut core = moulin(app).core();
        core.state = next;
        core.status = text;
    }
    tray::update(app);
    ui::settings::push_status(app);
    moulin(app).emit(Event::State(next));
}

/// The state "idle", with the text that tells how to dictate.
pub fn set_ready(app: &AppHandle) {
    let text = ready_text(&moulin(app).core().settings);
    set_state(app, State::Idle, text);
}

fn ready_text(settings: &Settings) -> String {
    if settings.hotkey.is_empty() {
        return "Ready. Use the menu or your system shortcut to dictate.".into();
    }
    match settings.mode {
        HotkeyMode::Hold => format!("Ready. Hold {} to dictate.", settings.hotkey),
        HotkeyMode::Toggle => format!("Ready. Press {} to dictate.", settings.hotkey),
    }
}

/// Shows an error in the tray and in the pop-up. When the pop-up is off, a notification shows it.
pub fn fail(app: &AppHandle, message: &str, hide_after: Duration) {
    set_state(app, State::Error, message);
    overlay::set(app, View::Error, message, Some(hide_after));
    if !moulin(app).core().settings.show_overlay {
        ui::notify(app, message);
    }
}

/// Makes the login item follow the setting. A run from the sources ("tauri dev") and the test modes do not change it.
pub fn apply_login_item(app: &AppHandle) {
    let me = moulin(app);
    if tauri::is_dev() || me.opts.mode != Mode::App {
        return;
    }
    let on = me.core().settings.open_at_login;
    if let Err(err) = platform::set_login_item(on) {
        eprintln!("Cannot change the login item: {err}");
    }
}

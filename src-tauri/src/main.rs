// Moulin: push the shortcut, speak, push it again. The text goes to the clipboard, and to the cursor if "paste" is on.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod capture;
mod chatgpt;
mod e2e;
mod paste;
mod platform;
mod settings;

use serde::Serialize;
use serde_json::{Value, json};
use settings::Settings;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{
    AppHandle, Emitter, Manager, RunEvent, State as Managed, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Wry,
    async_runtime::{JoinHandle, spawn, spawn_blocking},
    image::Image,
    menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    utils::config::BackgroundThrottlingPolicy,
    webview::{PageLoadEvent, PermissionResponse},
};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::broadcast;

const IS_MAC: bool = cfg!(target_os = "macos");
const MAX_RECORDING: Duration = Duration::from_secs(5 * 60); // Uploads of more than 326 s are not tested.
// Hold mode: a press shorter than this cancels the dictation, so that no empty audio goes to ChatGPT.
const SHORT_PRESS: Duration = Duration::from_millis(300);
const TRAY: &str = "main";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    App,
    E2e,
    Capture,
}

pub struct Options {
    pub mode: Mode,
    /// "Moulin --toggle" starts or stops a dictation. A system shortcut can run it (for example on Wayland).
    pub toggle: bool,
    pub hold: bool,
    pub sample: PathBuf,
    pub out_dir: PathBuf,
}

fn options() -> Options {
    let args: Vec<String> = std::env::args().collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let value = |name: &str| args.iter().find_map(|a| a.strip_prefix(&format!("--{name}=")).map(PathBuf::from));
    let cwd = std::env::current_dir().unwrap_or_default();
    // "tauri dev" runs the app in src-tauri, so the defaults also look in the parent folder.
    let root = if cwd.ends_with("src-tauri") { cwd.join("..") } else { cwd };
    Options {
        mode: if has("--e2e") {
            Mode::E2e
        } else if has("--capture") {
            Mode::Capture
        } else {
            Mode::App
        },
        toggle: has("--toggle"),
        hold: has("--hold"),
        sample: value("sample").unwrap_or_else(|| root.join("fixtures").join("fr-sample.wav")),
        out_dir: value("out").unwrap_or_else(|| root.join("artifacts")),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Idle,
    Recording,
    Transcribing,
    Error,
}

/// The E2E test listens to these events.
#[derive(Clone, Debug)]
pub enum Event {
    State(State),
    Overlay { view: String, text: String, visible: bool },
    Started,
    MicError(String),
    Audio { wav: Arc<Vec<u8>>, duration_ms: u64 },
    Result(Value),
}

struct Core {
    settings: Settings,
    state: State,
    status: String,
    signed_in: bool,
    hotkey_error: Option<String>,
    /// The last recording. The app keeps it until its text is in the clipboard.
    pending: Option<Arc<audio::Recording>>,
    /// The last texts of this session, for the tray menu.
    recent: Vec<String>,
    stop_at: u64,
    starting: bool,
    recorder: Option<audio::Recorder>,
    /// A stop or a cancel that comes while the microphone opens. It applies when the recorder is ready.
    stop_asked: Option<Ask>,
    /// The number of the current recording. A late timer or a late recorder of an old recording does nothing.
    generation: u64,
    /// Hold mode: the time of the key-down, until the key-up.
    held_since: Option<Instant>,
    overlay_shown: bool,
    spin_frame: usize,
    max_timer: Option<JoinHandle<()>>,
    spin_timer: Option<JoinHandle<()>>,
    hide_timer: Option<JoinHandle<()>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ask {
    Stop,
    Cancel,
}

struct TrayImages {
    idle: Image<'static>,
    busy: [Image<'static>; 4],
    recording: Image<'static>,
    error: Image<'static>,
}

// Tray images: black template images on macOS (the menu bar colors them), colored tiles on Windows and Linux.
// The images have two times the size of the tray, so they are sharp on a Retina screen. The system scales them.
#[cfg(target_os = "macos")]
macro_rules! tray_png {
    ($name:literal) => {
        Image::from_bytes(include_bytes!(concat!("../icons/tray/mac-", $name, ".png"))).expect("a valid tray image")
    };
}
#[cfg(not(target_os = "macos"))]
macro_rules! tray_png {
    ($name:literal) => {
        Image::from_bytes(include_bytes!(concat!("../icons/tray/tile-", $name, ".png"))).expect("a valid tray image")
    };
}

impl TrayImages {
    fn load() -> Self {
        Self {
            idle: tray_png!("idle"),
            busy: [tray_png!("busy0"), tray_png!("busy1"), tray_png!("busy2"), tray_png!("busy3")],
            recording: tray_png!("recording"),
            error: tray_png!("error"),
        }
    }
}

pub struct Moulin {
    core: Mutex<Core>,
    pub opts: Options,
    pub events: broadcast::Sender<Event>,
    folder: PathBuf,
    images: TrayImages,
}

impl Moulin {
    fn core(&self) -> MutexGuard<'_, Core> {
        self.core.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn settings_file(&self) -> PathBuf {
        self.folder.join("settings.json")
    }

    pub fn emit(&self, event: Event) {
        let _ = self.events.send(event);
    }

    pub fn settings(&self) -> Settings {
        self.core().settings.clone()
    }

    pub fn hotkey_error(&self) -> Option<String> {
        self.core().hotkey_error.clone()
    }
}

pub fn moulin(app: &AppHandle) -> Managed<'_, Moulin> {
    app.state::<Moulin>()
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn ready_text(settings: &Settings) -> String {
    if settings.hotkey.is_empty() {
        return "Ready. Use the menu or your system shortcut to dictate.".into();
    }
    if settings.hold() {
        format!("Ready. Hold {} to dictate.", settings.hotkey)
    } else {
        format!("Ready. Press {} to dictate.", settings.hotkey)
    }
}

fn ready(app: &AppHandle) -> String {
    ready_text(&moulin(app).core().settings)
}

fn save_settings(app: &AppHandle) {
    let me = moulin(app);
    let settings = me.core().settings.clone();
    if let Err(err) = settings::save(&me.settings_file(), &settings) {
        eprintln!("Cannot write {}: {err}", me.settings_file().display());
    }
}

fn wayland() -> bool {
    cfg!(target_os = "linux") && std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland")
}

// The tray shows the state. All tray changes run on the main thread, and each one reads the current state,
// so the last change always wins.
fn set_state(app: &AppHandle, next: State, text: impl Into<String>) {
    let text = text.into();
    let me = moulin(app);
    let spin = {
        let mut core = me.core();
        core.state = next;
        core.status = text.clone();
        core.spin_frame = 0;
        if next == State::Transcribing {
            core.spin_timer.is_none()
        } else {
            if let Some(timer) = core.spin_timer.take() {
                timer.abort();
            }
            false
        }
    };
    println!("[{}] {text}", serde_json::to_value(next).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default());
    if spin {
        let handle = app.clone();
        let timer = spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(120)).await;
                {
                    let me = moulin(&handle);
                    let mut core = me.core();
                    if core.state != State::Transcribing {
                        core.spin_timer = None;
                        return;
                    }
                    core.spin_frame = (core.spin_frame + 1) % 4;
                }
                let inner = handle.clone();
                let _ = handle.run_on_main_thread(move || render_icon(&inner));
            }
        });
        me.core().spin_timer = Some(timer);
    }
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        render_icon(&handle);
        if let Some(tray) = handle.tray_by_id(TRAY) {
            let status = moulin(&handle).core().status.clone();
            let _ = tray.set_tooltip(Some(format!("Moulin: {status}")));
            if let Ok(menu) = build_menu(&handle) {
                let _ = tray.set_menu(Some(menu));
            }
        }
    });
    push_status(app);
    me.emit(Event::State(next));
}

fn render_icon(app: &AppHandle) {
    let me = moulin(app);
    let (state, frame) = {
        let core = me.core();
        (core.state, core.spin_frame)
    };
    let (image, template) = match state {
        State::Transcribing => (&me.images.busy[frame], true),
        State::Recording => (&me.images.recording, false),
        State::Error => (&me.images.error, false),
        State::Idle => (&me.images.idle, true),
    };
    if let Some(tray) = app.tray_by_id(TRAY) {
        let _ = tray.set_icon_with_as_template(Some(image.clone()), IS_MAC && template);
    }
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let me = moulin(app);
    let (state, status, pending, recent, signed_in, hotkey) = {
        let core = me.core();
        (core.state, core.status.clone(), core.pending.is_some(), core.recent.clone(), core.signed_in, core.settings.hotkey.clone())
    };
    let busy = matches!(state, State::Recording | State::Transcribing);
    let registered = !hotkey.is_empty() && app.global_shortcut().is_registered(hotkey.as_str());
    let short =
        |t: &str| if t.chars().count() > 50 { format!("{}…", t.chars().take(50).collect::<String>().trim()) } else { t.to_string() };
    let item = |id: &str, text: &str, enabled: bool, accelerator: Option<&str>| {
        // The menu only shows the shortcut. A shortcut that the menu cannot show does not stop the menu.
        MenuItem::with_id(app, id, text, enabled, accelerator).or_else(|_| MenuItem::with_id(app, id, text, enabled, None::<&str>))
    };
    let mut items: Vec<Box<dyn IsMenuItem<Wry>>> = vec![
        Box::new(item("status", &status, false, None)?),
        Box::new(PredefinedMenuItem::separator(app)?),
        Box::new(item(
            "toggle",
            if state == State::Recording { "Stop dictation" } else { "Start dictation" },
            state != State::Transcribing,
            registered.then_some(hotkey.as_str()),
        )?),
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
        recent.iter().enumerate().map(|(i, t)| item(&format!("recent-{i}"), &short(t), true, None)).collect::<tauri::Result<_>>()?
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

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "toggle" => toggle(app),
        "cancel" => cancel_recording(app),
        "resend" => {
            let me = moulin(app);
            let ready = {
                let core = me.core();
                core.pending.is_some() && !matches!(core.state, State::Recording | State::Transcribing)
            };
            if ready {
                set_state(app, State::Transcribing, "Transcribing");
                set_overlay(app, "transcribing", "Transcribing", None);
                let handle = app.clone();
                spawn(async move { transcribe(&handle).await });
            }
        }
        "sign-in" => {
            let handle = app.clone();
            spawn(async move { sign_in(&handle).await });
        }
        "settings" => open_settings(app),
        "quit" => app.exit(0),
        _ => {
            let index = id.strip_prefix("recent-").and_then(|i| i.parse::<usize>().ok());
            let text = index.and_then(|i| moulin(app).core().recent.get(i).cloned());
            if let Some(text) = text {
                let _ = app.clipboard().write_text(text);
            }
        }
    }
}

fn create_overlay(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let window = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("overlay/overlay.html".into()))
        .title("Moulin")
        .inner_size(320.0, 60.0)
        .visible(false)
        .decorations(false)
        .transparent(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .shadow(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        // A click on the pop-up stops the dictation, also when Moulin is not the active app.
        .accept_first_mouse(true)
        .background_throttling(BackgroundThrottlingPolicy::Disabled)
        .on_permission_request(|_, _| PermissionResponse::Deny)
        .build()?;
    platform::setup_overlay(&window);
    Ok(window)
}

// The pop-up shows at the bottom center of the screen with the mouse pointer.
fn position_overlay(app: &AppHandle, overlay: &WebviewWindow) {
    let Ok(cursor) = app.cursor_position() else { return };
    let monitor = app.monitor_from_point(cursor.x, cursor.y).ok().flatten().or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    // macOS places windows in points, the other systems in pixels.
    if IS_MAC {
        let (x, y) = (f64::from(area.position.x) / scale, f64::from(area.position.y) / scale);
        let (width, height) = (f64::from(area.size.width) / scale, f64::from(area.size.height) / scale);
        let _ = overlay.set_position(tauri::LogicalPosition::new((x + (width - 320.0) / 2.0).round(), (y + height - 60.0 - 28.0).round()));
    } else {
        let (width, height) = ((320.0 * scale).round() as i32, (60.0 * scale).round() as i32);
        let x = area.position.x + (area.size.width as i32 - width) / 2;
        let y = area.position.y + area.size.height as i32 - height - (28.0 * scale).round() as i32;
        let _ = overlay.set_position(tauri::PhysicalPosition::new(x, y));
    }
}

fn set_overlay(app: &AppHandle, view: &str, text: &str, hide_after: Option<Duration>) {
    let me = moulin(app);
    let (show, was_shown, timer) = {
        let mut core = me.core();
        let show = view != "hidden" && core.settings.show_overlay;
        let was_shown = core.overlay_shown;
        core.overlay_shown = show;
        (show, was_shown, core.hide_timer.take())
    };
    if let Some(timer) = timer {
        timer.abort();
    }
    let Some(overlay) = app.get_webview_window("overlay") else { return };
    let _ = app.emit_to("overlay", "overlay:state", json!({ "state": view, "text": text }));
    if !show {
        let _ = overlay.hide();
    } else if !was_shown {
        position_overlay(app, &overlay);
        platform::show_inactive(&overlay);
    }
    if let Some(after) = hide_after {
        let handle = app.clone();
        me.core().hide_timer = Some(spawn(async move {
            tokio::time::sleep(after).await;
            set_overlay(&handle, "hidden", "", None);
        }));
    }
    me.emit(Event::Overlay { view: view.into(), text: text.into(), visible: show });
}

fn notify(app: &AppHandle, body: &str) {
    let _ = app.notification().builder().title("Moulin").body(body).show();
}

fn fail(app: &AppHandle, message: &str, hide_after: Duration) {
    set_state(app, State::Error, message);
    set_overlay(app, "error", message, Some(hide_after));
    if !moulin(app).core().settings.show_overlay {
        notify(app, message);
    }
}

/// Runs a step of the dictation on the main thread. The steps never overlap, so a stop cannot come between
/// the parts of a start. The shortcut handlers also return at once: on X11, the shortcut plugin calls them on its
/// own thread, and an unregister from that thread waits for itself.
fn on_main(app: &AppHandle, step: impl FnOnce(&AppHandle) + Send + 'static) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || step(&handle));
}

// Escape cancels a dictation. If another app holds Escape, the tray menu can still cancel.
fn register_escape(app: &AppHandle, on: bool) {
    let shortcuts = app.global_shortcut();
    if on && !shortcuts.is_registered("Escape") {
        let _ = shortcuts.on_shortcut("Escape", |app, _, event| {
            if event.state == ShortcutState::Pressed {
                on_main(app, cancel_recording);
            }
        });
    } else if !on && shortcuts.is_registered("Escape") {
        let _ = shortcuts.unregister("Escape");
    }
}

// macOS asks one time. A refusal needs a change in System Settings.
async fn microphone_allowed(app: &AppHandle) -> bool {
    if moulin(app).opts.mode != Mode::App {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        use platform::Access;
        match platform::microphone_access() {
            Access::Granted => true,
            Access::Undetermined => platform::request_microphone().await,
            Access::Denied => {
                platform::open_privacy_pane("Privacy_Microphone");
                false
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    true
}

// The checks before a dictation: the sign-in and the microphone permission. They can wait for a page or a prompt,
// so they run in a task. hold: the key of the hold mode started the dictation.
async fn start_recording(app: AppHandle, hold: bool) {
    let me = moulin(&app);
    let signed_in = {
        let mut core = me.core();
        if core.starting || matches!(core.state, State::Recording | State::Transcribing) {
            return;
        }
        core.starting = true;
        core.signed_in
    };
    if !signed_in {
        let page = chatgpt::start(&app).await;
        me.core().signed_in = page.signed_in;
        if !page.signed_in {
            me.core().starting = false;
            fail(&app, "Sign in to ChatGPT first.", Duration::from_secs(4));
            on_main(&app, open_settings);
            return;
        }
    }
    if !microphone_allowed(&app).await {
        me.core().starting = false;
        fail(&app, "Allow Moulin in System Settings > Privacy & Security > Microphone.", Duration::from_secs(6));
        return;
    }
    on_main(&app, move |app| begin_recording(app, hold));
}

// The start of a dictation, on the main thread. The pop-up shows at once, and the microphone opens in a task.
// In hold mode, a key that is up again means a short press: no dictation.
fn begin_recording(app: &AppHandle, hold: bool) {
    let me = moulin(app);
    let (generation, source) = {
        let mut core = me.core();
        core.starting = false;
        if matches!(core.state, State::Recording | State::Transcribing) || (hold && core.held_since.is_none()) {
            return;
        }
        core.generation += 1;
        core.stop_asked = None;
        let source = match me.opts.mode {
            Mode::E2e => audio::Source::Sample(me.opts.sample.clone()),
            _ => audio::Source::Microphone(core.settings.microphone_id.clone()),
        };
        (core.generation, source)
    };
    set_state(app, State::Recording, "Recording");
    set_overlay(app, "recording", "", None);
    register_escape(app, true);
    let handle = app.clone();
    let timer = spawn(async move {
        tokio::time::sleep(MAX_RECORDING).await;
        on_main(&handle, move |app| {
            if moulin(app).core().generation == generation {
                stop_recording(app);
            }
        });
    });
    if let Some(old) = me.core().max_timer.replace(timer) {
        old.abort();
    }
    let handle = app.clone();
    spawn(async move {
        let levels = handle.clone();
        let on_level = move |level: f32, ms: u64| {
            let _ = levels.emit_to("overlay", "overlay:level", json!({ "level": level, "ms": ms }));
        };
        let started = spawn_blocking(move || audio::Recorder::start(source, on_level)).await.unwrap_or_else(|err| Err(err.to_string()));
        on_main(&handle, move |app| opened(app, generation, started));
    });
}

// The microphone is open, or it failed. A stop or a cancel during the opening applies now.
fn opened(app: &AppHandle, generation: u64, started: Result<audio::Recorder, String>) {
    let me = moulin(app);
    let (current, asked) = {
        let mut core = me.core();
        let current = core.generation == generation;
        (current, if current { core.stop_asked.take() } else { None })
    };
    match started {
        Ok(recorder) if !current => recorder.cancel(),
        Ok(recorder) => match asked {
            Some(Ask::Cancel) => recorder.cancel(),
            Some(Ask::Stop) => finish_recording(app, recorder),
            None => {
                me.core().recorder = Some(recorder);
                me.emit(Event::Started);
            }
        },
        Err(_) if !current || asked == Some(Ask::Cancel) => {}
        Err(message) => {
            if let Some(timer) = me.core().max_timer.take() {
                timer.abort();
            }
            register_escape(app, false);
            fail(app, &format!("Microphone error: {message}"), Duration::from_secs(4));
            me.emit(Event::MicError(message));
        }
    }
}

// Stops the recorder in a task (it waits for the audio thread), then uploads the audio.
fn finish_recording(app: &AppHandle, recorder: audio::Recorder) {
    let handle = app.clone();
    spawn(async move {
        let recording = spawn_blocking(move || recorder.stop()).await.ok().flatten();
        let Some(recording) = recording else {
            on_main(&handle, |app| fail(app, "Microphone error: the recording is empty.", Duration::from_secs(4)));
            return;
        };
        let me = moulin(&handle);
        let recording = Arc::new(recording);
        me.core().pending = Some(recording.clone());
        me.emit(Event::Audio { wav: Arc::new(recording.wav.clone()), duration_ms: recording.duration_ms });
        transcribe(&handle).await;
    });
}

fn stop_recording(app: &AppHandle) {
    let me = moulin(app);
    let (recorder, timer) = {
        let mut core = me.core();
        if core.state != State::Recording {
            return;
        }
        core.stop_at = now_ms();
        let recorder = core.recorder.take();
        if recorder.is_none() {
            core.stop_asked = Some(Ask::Stop);
        }
        (recorder, core.max_timer.take())
    };
    if let Some(timer) = timer {
        timer.abort();
    }
    register_escape(app, false);
    set_state(app, State::Transcribing, "Transcribing");
    set_overlay(app, "transcribing", "Transcribing", None);
    if let Some(recorder) = recorder {
        finish_recording(app, recorder);
    }
}

fn cancel_recording(app: &AppHandle) {
    let me = moulin(app);
    let (recorder, timer) = {
        let mut core = me.core();
        if core.state != State::Recording {
            return;
        }
        let recorder = core.recorder.take();
        if recorder.is_none() {
            core.stop_asked = Some(Ask::Cancel);
        }
        (recorder, core.max_timer.take())
    };
    if let Some(timer) = timer {
        timer.abort();
    }
    register_escape(app, false);
    if let Some(recorder) = recorder {
        recorder.cancel();
    }
    set_state(app, State::Idle, ready(app));
    set_overlay(app, "hidden", "", None);
}

fn toggle(app: &AppHandle) {
    if moulin(app).core().state == State::Recording {
        stop_recording(app);
    } else {
        spawn(start_recording(app.clone(), false));
    }
}

// macOS: a click on the pop-up makes Moulin the active app, so the paste would go to Moulin.
// When Moulin hides, macOS gives the front back to the previous app. Then the pop-up shows again without activation.
async fn stop_from_click(app: AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let settings_focused = app.get_webview_window("settings").and_then(|w| w.is_focused().ok()).unwrap_or(false);
        if !settings_focused {
            let _ = app.hide();
            tokio::time::sleep(Duration::from_millis(40)).await;
            if moulin(&app).core().state != State::Recording {
                return; // Esc or the time limit stopped the dictation during the wait.
            }
            if let Some(overlay) = app.get_webview_window("overlay") {
                platform::show_inactive(&overlay);
            }
        }
    }
    on_main(&app, stop_recording);
}

// macOS needs the Accessibility permission to paste. Without it, the text stays in the clipboard.
async fn paste_text(app: &AppHandle) -> bool {
    #[cfg(target_os = "macos")]
    if !platform::accessibility(false) {
        platform::accessibility(true);
        notify(app, "Allow Moulin in System Settings > Privacy & Security > Accessibility to paste. The text is in the clipboard.");
        return false;
    }
    tokio::time::sleep(Duration::from_millis(60)).await;
    match spawn_blocking(paste::paste).await.unwrap_or_else(|err| Err(err.to_string())) {
        Ok(()) => true,
        Err(message) => {
            notify(app, &format!("Moulin cannot paste ({message}). The text is in the clipboard."));
            false
        }
    }
}

// The upload of the pending recording. The caller sets the state "transcribing" first.
async fn transcribe(app: &AppHandle) {
    let me = moulin(app);
    let Some(recording) = me.core().pending.clone() else { return };
    let mut result = chatgpt::transcribe(app, &recording).await;
    let full = result.text.clone().unwrap_or_default();
    let text = full.trim().to_string();
    if !text.is_empty()
        && let Err(err) = app.clipboard().write_text(full.clone())
    {
        result.ok = false;
        result.message = format!("Moulin cannot write to the clipboard: {err}");
    }
    let mut clipboard_at = None;
    if result.ok {
        clipboard_at = Some(now_ms());
        let mut core = me.core();
        core.pending = None;
        if !text.is_empty() {
            core.recent.insert(0, full.clone());
            core.recent.truncate(5);
        }
    }
    let pasted = result.ok && !text.is_empty() && me.core().settings.auto_paste && me.opts.mode == Mode::App && paste_text(app).await;
    if result.status == 401 {
        me.core().signed_in = false;
    }
    let mut payload = serde_json::to_value(&result).unwrap_or_default();
    payload["clipboardAt"] = json!(clipboard_at);
    payload["stopAt"] = json!(me.core().stop_at);
    let done = if text.is_empty() {
        "No speech"
    } else if pasted {
        "Pasted"
    } else {
        "Copied"
    };
    on_main(app, move |app| {
        if result.ok {
            set_state(app, State::Idle, ready(app));
            set_overlay(app, "done", done, Some(Duration::from_millis(1300)));
        } else {
            fail(app, &result.message, Duration::from_secs(4));
            if result.show_window {
                chatgpt::show(app);
            }
        }
        moulin(app).emit(Event::Result(payload));
    });
}

// An empty shortcut means no Moulin shortcut: a system shortcut runs "Moulin --toggle".
fn register_hotkey(app: &AppHandle, accelerator: &str) -> Option<String> {
    if accelerator.is_empty() {
        return None;
    }
    // The shortcut plugin uses X11. On Wayland, an X11 app gets the keys only while one of its windows has the focus.
    if wayland() {
        return Some(format!(
            "This desktop does not give {accelerator} to Moulin. Add a system shortcut that runs Moulin with --toggle, and remove this one."
        ));
    }
    let Ok(shortcut) = accelerator.parse::<Shortcut>() else { return Some(format!("{accelerator} is not a valid shortcut.")) };
    let handler = |app: &AppHandle, _: &Shortcut, event: tauri_plugin_global_shortcut::ShortcutEvent| {
        let state = event.state;
        on_main(app, move |app| on_shortcut(app, state));
    };
    match app.global_shortcut().on_shortcut(shortcut, handler) {
        Ok(()) => None,
        Err(_) => Some(format!("{accelerator} is in use by another app. Choose a different shortcut.")),
    }
}

// The shortcut starts or stops a dictation (toggle). In hold mode, it starts the dictation, and the key-up stops it.
// The shortcut plugin reports the key-up on macOS, Windows, and X11, so the hold mode needs no key hook.
fn on_shortcut(app: &AppHandle, state: ShortcutState) {
    let me = moulin(app);
    let hold = me.core().settings.hold();
    if !hold {
        if state == ShortcutState::Pressed {
            toggle(app);
        }
        return;
    }
    match state {
        ShortcutState::Pressed => {
            {
                let mut core = me.core();
                if core.held_since.is_some() {
                    return; // The key repeats while it stays down.
                }
                core.held_since = Some(Instant::now());
            }
            spawn(start_recording(app.clone(), true));
        }
        ShortcutState::Released => {
            let (short, recording) = {
                let mut core = me.core();
                let Some(since) = core.held_since.take() else { return };
                (since.elapsed() < SHORT_PRESS, core.state == State::Recording)
            };
            // During the checks before the start, begin_recording sees that the key is up. While the microphone
            // opens, the state is already "recording", and the stop or the cancel waits for the recorder.
            if recording && short {
                cancel_recording(app);
            } else if recording {
                stop_recording(app);
            }
        }
    }
}

// Keep the old shortcut if the new one does not register.
fn change_hotkey(app: &AppHandle, next: &str) -> Option<String> {
    let me = moulin(app);
    let old = me.core().settings.hotkey.clone();
    if !old.is_empty() && app.global_shortcut().is_registered(old.as_str()) {
        let _ = app.global_shortcut().unregister(old.as_str());
    }
    if let Some(error) = register_hotkey(app, next) {
        register_hotkey(app, &old);
        return Some(error);
    }
    me.core().settings.hotkey = next.into();
    None
}

fn packaged() -> bool {
    !tauri::is_dev()
}

fn apply_login_item(app: &AppHandle) {
    let me = moulin(app);
    if !packaged() || me.opts.mode != Mode::App {
        return;
    }
    let on = me.core().settings.open_at_login;
    if let Err(err) = platform::set_login_item(on) {
        eprintln!("Cannot change the login item: {err}");
    }
}

async fn sign_in(app: &AppHandle) {
    let state = moulin(app).core().state;
    set_state(app, state, "Sign in to ChatGPT in the window");
    let ok = chatgpt::sign_in(app).await;
    moulin(app).core().signed_in = ok;
    if ok {
        set_state(app, State::Idle, ready(app));
    } else {
        set_state(app, State::Error, "Not signed in to ChatGPT");
    }
}

fn snapshot(app: &AppHandle) -> Value {
    let me = moulin(app);
    let core = me.core();
    json!({
        "settings": core.settings,
        "state": core.state,
        "status": core.status,
        "signedIn": core.signed_in,
        "hotkeyError": core.hotkey_error,
        "version": app.package_info().version.to_string(),
        "platform": std::env::consts::OS,
        "accessibility": accessibility(),
    })
}

fn accessibility() -> bool {
    #[cfg(target_os = "macos")]
    return platform::accessibility(false);
    #[cfg(not(target_os = "macos"))]
    true
}

fn push_status(app: &AppHandle) {
    if app.get_webview_window("settings").is_some() {
        let _ = app.emit_to("settings", "status", snapshot(app));
    }
}

fn open_settings(app: &AppHandle) {
    if moulin(app).opts.mode != Mode::App {
        return;
    }
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let builder = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings/settings.html".into()))
        .title("Moulin")
        .inner_size(560.0, 800.0)
        .resizable(false)
        .maximizable(false)
        .visible(false)
        .on_permission_request(|_, _| PermissionResponse::Deny)
        // Show the window when its page is ready, so that it never shows empty.
        .on_page_load(|window, payload| {
            if payload.event() == PageLoadEvent::Finished {
                let _ = window.show();
                let _ = window.set_focus();
            }
        });
    #[cfg(target_os = "macos")]
    let builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
    if let Err(err) = builder.build() {
        eprintln!("Cannot open the settings: {err}");
    }
}

#[tauri::command]
fn settings_get(app: AppHandle) -> Value {
    snapshot(&app)
}

#[tauri::command]
fn settings_set(app: AppHandle, patch: Value) -> Value {
    let Some(patch) = patch.as_object() else { return Value::Null };
    let me = moulin(&app);
    let current = me.core().settings.hotkey.clone();
    if let Some(hotkey) = patch.get("hotkey").and_then(Value::as_str)
        && hotkey != current
    {
        let error = change_hotkey(&app, hotkey);
        me.core().hotkey_error = error;
    }
    {
        let mut core = me.core();
        let flag = |key: &str| patch.get(key).and_then(Value::as_bool);
        if let Some(on) = flag("autoPaste") {
            core.settings.auto_paste = on;
        }
        if let Some(on) = flag("openAtLogin") {
            core.settings.open_at_login = on;
        }
        if let Some(on) = flag("showOverlay") {
            core.settings.show_overlay = on;
        }
        if let Some(mode @ ("toggle" | "hold")) = patch.get("mode").and_then(Value::as_str) {
            core.settings.mode = mode.into();
        }
        if let Some(id) = patch.get("microphoneId").and_then(Value::as_str) {
            core.settings.microphone_id = id.into();
        }
    }
    save_settings(&app);
    apply_login_item(&app);
    // A good shortcut also clears an old error, for example "This desktop does not give F5 to Moulin".
    let (state, signed_in, hotkey_error) = {
        let core = me.core();
        (core.state, core.signed_in, core.hotkey_error.is_some())
    };
    if state == State::Idle || (state == State::Error && signed_in && !hotkey_error) {
        set_state(&app, State::Idle, ready(&app));
    }
    snapshot(&app)
}

// While the user types a new shortcut, the old one must not start a dictation.
#[tauri::command]
fn settings_capture(app: AppHandle, on: bool) {
    let hotkey = moulin(&app).core().settings.hotkey.clone();
    if hotkey.is_empty() {
        return;
    }
    let registered = app.global_shortcut().is_registered(hotkey.as_str());
    if on && registered {
        let _ = app.global_shortcut().unregister(hotkey.as_str());
    } else if !on && !registered {
        register_hotkey(&app, &hotkey);
    }
}

#[tauri::command]
async fn account_sign_in(app: AppHandle) -> Value {
    sign_in(&app).await;
    snapshot(&app)
}

// WebKit shows no window.confirm() dialog, so Moulin asks with a dialog of the system.
#[tauri::command]
async fn account_sign_out(app: AppHandle) -> Value {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut dialog = app
        .dialog()
        .message("Sign out of ChatGPT in Moulin? Moulin deletes its cookies for chatgpt.com.")
        .title("Moulin")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom("Sign out".into(), "Cancel".into()));
    if let Some(window) = app.get_webview_window("settings") {
        dialog = dialog.parent(&window);
    }
    dialog.show(move |ok| {
        let _ = tx.send(ok);
    });
    if !rx.await.unwrap_or(false) {
        return Value::Null;
    }
    let page = chatgpt::sign_out(&app).await;
    moulin(&app).core().signed_in = page.signed_in;
    set_state(&app, State::Error, "Not signed in to ChatGPT");
    snapshot(&app)
}

#[tauri::command]
fn open_accessibility() {
    #[cfg(target_os = "macos")]
    if !platform::accessibility(true) {
        platform::open_privacy_pane("Privacy_Accessibility");
    }
}

#[tauri::command]
fn microphones() -> Vec<audio::Microphone> {
    audio::microphones()
}

#[tauri::command]
fn overlay_clicked(app: AppHandle) {
    if moulin(&app).core().state == State::Recording {
        spawn(stop_from_click(app));
    }
}

// First launch on macOS: ask for the microphone and for Accessibility now, so that the first dictation records and pastes.
async fn ask_permissions_once(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        microphone_allowed(app).await;
        if moulin(app).core().settings.auto_paste {
            platform::accessibility(true);
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

async fn startup(app: AppHandle, first_run: bool) {
    let me = moulin(&app);
    if me.opts.mode == Mode::Capture {
        capture::run(app.clone()).await;
        return;
    }
    set_state(&app, State::Idle, "Starting");
    let hotkey = me.core().settings.hotkey.clone();
    let error = register_hotkey(&app, &hotkey);
    me.core().hotkey_error = error.clone();
    if me.opts.mode == Mode::E2e && me.opts.hold {
        me.core().settings.mode = "hold".into(); // Not saved: the test only.
    }
    apply_login_item(&app);
    let page = chatgpt::start(&app).await;
    me.core().signed_in = page.signed_in;
    if page.status == 403 {
        set_state(&app, State::Error, "Cloudflare asks for a check. Complete it in the ChatGPT window.");
        chatgpt::show(&app);
    } else if !page.signed_in {
        set_state(&app, State::Error, "Not signed in to ChatGPT");
        open_settings(&app);
    } else if let Some(error) = error {
        set_state(&app, State::Error, error);
    } else {
        set_state(&app, State::Idle, ready(&app));
    }
    if me.opts.mode == Mode::App && first_run {
        save_settings(&app);
        open_settings(&app);
        ask_permissions_once(&app).await;
    }
    if me.opts.mode == Mode::App && me.opts.toggle {
        on_main(&app, toggle); // A system shortcut started Moulin: start the dictation now.
    }
    if me.opts.mode == Mode::E2e {
        e2e::run(app.clone(), page).await;
    }
}

fn main() {
    let opts = options();
    let mode = opts.mode;
    let opts = Mutex::new(Some(opts));
    tauri::Builder::default()
        // A second start sends its request to the running Moulin: --toggle starts or stops a dictation, a plain start opens the settings.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if argv.iter().any(|a| a == "--toggle") {
                on_main(app, toggle);
            } else {
                on_main(app, open_settings);
            }
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(chatgpt::ChatGpt::new())
        .invoke_handler(tauri::generate_handler![
            overlay_clicked,
            settings_get,
            settings_set,
            settings_capture,
            account_sign_in,
            account_sign_out,
            open_accessibility,
            microphones,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let opts = opts.lock().unwrap().take().expect("the options");
            let folder = app.path().app_config_dir()?;
            let file = folder.join("settings.json");
            let first_run = !file.exists();
            let settings = settings::load(&file);
            app.manage(Moulin {
                core: Mutex::new(Core {
                    settings,
                    state: State::Idle,
                    status: "Starting".into(),
                    signed_in: false,
                    hotkey_error: None,
                    pending: None,
                    recent: Vec::new(),
                    stop_at: 0,
                    starting: false,
                    recorder: None,
                    stop_asked: None,
                    generation: 0,
                    held_since: None,
                    overlay_shown: false,
                    spin_frame: 0,
                    max_timer: None,
                    spin_timer: None,
                    hide_timer: None,
                }),
                opts,
                events: broadcast::channel(256).0,
                folder,
                images: TrayImages::load(),
            });
            if mode == Mode::Capture {
                chatgpt::create(&handle, true, Some(capture::SCRIPT))?;
            } else {
                #[cfg(target_os = "macos")]
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                let menu = build_menu(&handle)?;
                TrayIconBuilder::with_id(TRAY)
                    .icon(moulin(&handle).images.idle.clone())
                    .icon_as_template(IS_MAC)
                    .tooltip("Moulin: Starting")
                    .menu(&menu)
                    .show_menu_on_left_click(true)
                    .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
                    .build(app)?;
                create_overlay(&handle)?;
                chatgpt::create(&handle, false, None)?;
            }
            spawn(startup(handle, first_run));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Moulin cannot start")
        .run(|app, event| match event {
            // Moulin stays in the tray when the last window closes.
            RunEvent::ExitRequested { api, code: None, .. } => api.prevent_exit(),
            RunEvent::Exit => {
                app.state::<chatgpt::ChatGpt>().quit();
                let _ = app.global_shortcut().unregister_all();
            }
            // A click on Moulin in the Finder or the Dock while it runs: open the settings.
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => open_settings(app),
            _ => {}
        });
}

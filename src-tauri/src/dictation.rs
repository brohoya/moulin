//! A dictation: the checks, the recording, the upload, the clipboard, and the paste.
//!
//! The start, the stop, and the cancel run on the main thread (see `on_main`), so they never overlap.
//! The steps that wait (the checks, the microphone, the upload) run in tasks.
use crate::{
    account,
    app::{self, Event, State, Transcription, fail, moulin, now_ms, on_main, set_ready, set_state},
    audio,
    chatgpt::{self, check},
    hotkey,
    options::Mode,
    paste, platform,
    ui::{
        self,
        overlay::{self, View},
    },
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tauri::{
    AppHandle,
    async_runtime::{JoinHandle, spawn, spawn_blocking},
};
use tauri_plugin_clipboard_manager::ClipboardExt;

/// A dictation stops after this time. Uploads of more than 326 s are not tested.
const MAX_RECORDING: Duration = Duration::from_secs(5 * 60);
/// Hold mode: a press shorter than this cancels the dictation, so that no empty audio goes to ChatGPT.
const SHORT_PRESS: Duration = Duration::from_millis(300);
/// The number of recent texts in the tray menu.
const RECENT: usize = 5;

/// A stop or a cancel that comes while the microphone opens.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ask {
    Stop,
    Cancel,
}

/// The current dictation, in the shared state.
#[derive(Default)]
pub struct Session {
    /// The checks before the start run.
    starting: bool,
    /// The number of the current recording. A late timer or a late recorder of an old recording does nothing.
    generation: u64,
    recorder: Option<audio::Recorder>,
    /// A stop or a cancel that comes while the microphone opens. It applies when the recorder is ready.
    asked: Option<Ask>,
    /// Hold mode: the time of the key-down, until the key-up.
    held_since: Option<Instant>,
    /// The time limit of the recording.
    max_timer: Option<JoinHandle<()>>,
    /// The time of the last stop, for the E2E report.
    stop_at: u64,
    /// The last recording. Moulin keeps it until its text is in the clipboard.
    pending: Option<Arc<audio::Recording>>,
}

impl Session {
    /// True if a recording can go to ChatGPT again, after a failed upload.
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// The end of the recording, for a stop or a cancel: the time limit stops. Returns the recorder. While the
    /// microphone opens, there is no recorder yet, so `ask` waits for it.
    fn end(&mut self, ask: Ask) -> Option<audio::Recorder> {
        if let Some(timer) = self.max_timer.take() {
            timer.abort();
        }
        let recorder = self.recorder.take();
        if recorder.is_none() {
            self.asked = Some(ask);
        }
        recorder
    }
}

/// Starts or stops a dictation: the shortcut in toggle mode, the menu, and "Moulin --toggle".
pub fn toggle(app: &AppHandle) {
    if moulin(app).core().state == State::Recording {
        stop(app);
    } else {
        spawn(start(app.clone(), false));
    }
}

/// Hold mode: the key-down starts a dictation.
pub fn press(app: &AppHandle) {
    {
        let mut core = moulin(app).core();
        if core.dictation.held_since.is_some() {
            return; // The key repeats while it stays down.
        }
        core.dictation.held_since = Some(Instant::now());
    }
    spawn(start(app.clone(), true));
}

/// Hold mode: the key-up stops the dictation. After a short press, it cancels the dictation.
pub fn release(app: &AppHandle) {
    let (short, recording) = {
        let mut core = moulin(app).core();
        let Some(since) = core.dictation.held_since.take() else { return };
        (since.elapsed() < SHORT_PRESS, core.state == State::Recording)
    };
    // During the checks before the start, `begin` sees that the key is up. While the microphone opens, the state
    // is already "recording", and the stop or the cancel waits for the recorder.
    if recording && short {
        cancel(app);
    } else if recording {
        stop(app);
    }
}

/// Stops the recording and uploads it.
pub fn stop(app: &AppHandle) {
    let recorder = {
        let mut core = moulin(app).core();
        if core.state != State::Recording {
            return;
        }
        core.dictation.stop_at = now_ms();
        core.dictation.end(Ask::Stop)
    };
    hotkey::set_escape(app, false);
    show_transcribing(app);
    if let Some(recorder) = recorder {
        finish(app, recorder);
    }
}

/// Stops the recording and drops it.
pub fn cancel(app: &AppHandle) {
    let recorder = {
        let mut core = moulin(app).core();
        if core.state != State::Recording {
            return;
        }
        core.dictation.end(Ask::Cancel)
    };
    hotkey::set_escape(app, false);
    if let Some(recorder) = recorder {
        recorder.cancel();
    }
    set_ready(app);
    overlay::hide(app);
}

/// Uploads the last recording again, after a failed upload.
pub fn resend(app: &AppHandle) {
    let ready = {
        let core = moulin(app).core();
        core.dictation.pending.is_some() && !core.state.busy()
    };
    if ready {
        show_transcribing(app);
        let app = app.clone();
        spawn(async move { transcribe(&app).await });
    }
}

fn show_transcribing(app: &AppHandle) {
    set_state(app, State::Transcribing, "Transcribing");
    overlay::set(app, View::Transcribing, "Transcribing", None);
}

/// The checks before a dictation: the sign-in and the microphone permission. They can wait for a page or a prompt,
/// so they run in a task. `hold`: the key of the hold mode started the dictation.
async fn start(app: AppHandle, hold: bool) {
    let signed_in = {
        let mut core = moulin(&app).core();
        if core.dictation.starting || core.state.busy() {
            return;
        }
        core.dictation.starting = true;
        core.signed_in
    };
    if !signed_in {
        let page = account::check(&app).await;
        if !page.signed_in {
            moulin(&app).core().dictation.starting = false;
            if page.status == 403 {
                app::refused(&app, check::PAGE_REFUSED, check::PAGE_NOTICE);
            } else {
                fail(&app, "Sign in to ChatGPT first.", Duration::from_secs(4));
                on_main(&app, ui::settings::open);
            }
            return;
        }
    }
    // The test modes record no microphone.
    if moulin(&app).opts.mode == Mode::App && !platform::microphone_allowed().await {
        moulin(&app).core().dictation.starting = false;
        fail(&app, "Allow Moulin in System Settings > Privacy & Security > Microphone.", Duration::from_secs(6));
        return;
    }
    on_main(&app, move |app| begin(app, hold));
}

/// The start of a dictation, on the main thread. The pop-up shows at once, and the microphone opens in a task.
/// In hold mode, a key that is up again means a short press: no dictation.
fn begin(app: &AppHandle, hold: bool) {
    let me = moulin(app);
    let (generation, source) = {
        let mut core = me.core();
        core.dictation.starting = false;
        if core.state.busy() || (hold && core.dictation.held_since.is_none()) {
            return;
        }
        core.dictation.generation += 1;
        core.dictation.asked = None;
        let source = match me.opts.mode {
            Mode::E2e => audio::Source::Sample(me.opts.sample.clone()),
            _ => audio::Source::Microphone(core.settings.microphone_id.clone()),
        };
        (core.dictation.generation, source)
    };
    set_state(app, State::Recording, "Recording");
    overlay::set(app, View::Recording, "", None);
    hotkey::set_escape(app, true);
    let old_limit = me.core().dictation.max_timer.replace(spawn(time_limit(app.clone(), generation)));
    if let Some(old_limit) = old_limit {
        old_limit.abort();
    }
    spawn(open_microphone(app.clone(), generation, source));
}

/// Stops the recording at the time limit, if it is still the same recording.
async fn time_limit(app: AppHandle, generation: u64) {
    tokio::time::sleep(MAX_RECORDING).await;
    on_main(&app, move |app| {
        if moulin(app).core().dictation.generation == generation {
            stop(app);
        }
    });
}

/// Opens the microphone (it can take a moment), then gives the recorder or the error to the main thread.
async fn open_microphone(app: AppHandle, generation: u64, source: audio::Source) {
    let levels = app.clone();
    let on_level = move |level: f32, ms: u64| overlay::level(&levels, level, ms);
    let started = spawn_blocking(move || audio::Recorder::start(source, on_level)).await.unwrap_or_else(|err| Err(err.to_string()));
    on_main(&app, move |app| opened(app, generation, started));
}

/// The microphone is open, or it failed. A stop or a cancel during the opening applies now.
fn opened(app: &AppHandle, generation: u64, started: Result<audio::Recorder, String>) {
    let me = moulin(app);
    let (current, asked) = {
        let mut core = me.core();
        let current = core.dictation.generation == generation;
        (current, if current { core.dictation.asked.take() } else { None })
    };
    match started {
        Ok(recorder) if !current => recorder.cancel(),
        Ok(recorder) => match asked {
            Some(Ask::Cancel) => recorder.cancel(),
            Some(Ask::Stop) => finish(app, recorder),
            None => {
                me.core().dictation.recorder = Some(recorder);
                me.emit(Event::Started);
            }
        },
        Err(_) if !current || asked == Some(Ask::Cancel) => {}
        Err(message) => {
            let limit = me.core().dictation.max_timer.take();
            if let Some(limit) = limit {
                limit.abort();
            }
            hotkey::set_escape(app, false);
            fail(app, &format!("Microphone error: {message}"), Duration::from_secs(4));
            me.emit(Event::MicError(message));
        }
    }
}

/// Stops the recorder in a task (it waits for the audio thread), then uploads the audio.
fn finish(app: &AppHandle, recorder: audio::Recorder) {
    let app = app.clone();
    spawn(async move {
        let recording = spawn_blocking(move || recorder.stop()).await.ok().flatten();
        let Some(recording) = recording else {
            on_main(&app, |app| fail(app, "Microphone error: the recording is empty.", Duration::from_secs(4)));
            return;
        };
        let recording = Arc::new(recording);
        moulin(&app).core().dictation.pending = Some(recording.clone());
        moulin(&app).emit(Event::Audio(recording));
        transcribe(&app).await;
    });
}

/// Uploads the pending recording, and puts the text in the clipboard. The caller shows "Transcribing" first.
async fn transcribe(app: &AppHandle) {
    let me = moulin(app);
    let Some(recording) = me.core().dictation.pending.clone() else { return };
    let on_check = |step| match step {
        Some(step) => app::show_check(app, step),
        None => show_transcribing(app),
    };
    let mut outcome = chatgpt::transcribe(app, &recording, on_check).await;
    let text = outcome.text.clone().unwrap_or_default();
    let speech = !text.trim().is_empty();
    if speech && let Err(err) = app.clipboard().write_text(text.clone()) {
        outcome.ok = false;
        outcome.message = format!("Moulin cannot write to the clipboard: {err}");
    }
    let mut clipboard_at = None;
    if outcome.ok {
        clipboard_at = Some(now_ms());
        let mut core = me.core();
        core.dictation.pending = None;
        if speech {
            core.recent.insert(0, text);
            core.recent.truncate(RECENT);
        }
    }
    let paste = outcome.ok && speech && me.core().settings.auto_paste && me.opts.mode == Mode::App;
    let pasted = paste && paste_text(app).await;
    if outcome.status == 401 {
        me.core().signed_in = false;
    }
    let label = if !speech {
        "No speech"
    } else if pasted {
        "Pasted"
    } else {
        "Copied"
    };
    let transcription = Transcription { outcome, clipboard_at, stop_at: me.core().dictation.stop_at };
    on_main(app, move |app| {
        let outcome = &transcription.outcome;
        if outcome.ok {
            set_ready(app);
            overlay::set(app, View::Done, label, Some(Duration::from_millis(1300)));
        } else if outcome.challenge {
            app::refused(app, &outcome.message, check::UPLOAD_NOTICE);
        } else {
            fail(app, &outcome.message, Duration::from_secs(4));
        }
        moulin(app).emit(Event::Transcription(transcription));
    });
}

/// Pastes the clipboard at the cursor. macOS needs the Accessibility permission to paste. Without it, the text
/// stays in the clipboard.
async fn paste_text(app: &AppHandle) -> bool {
    if !platform::accessibility(false) {
        platform::accessibility(true);
        ui::notify(app, "Allow Moulin in System Settings > Privacy & Security > Accessibility to paste. The text is in the clipboard.");
        return false;
    }
    tokio::time::sleep(Duration::from_millis(60)).await;
    match spawn_blocking(paste::paste).await.unwrap_or_else(|err| Err(err.to_string())) {
        Ok(()) => true,
        Err(message) => {
            ui::notify(app, &format!("Moulin cannot paste ({message}). The text is in the clipboard."));
            false
        }
    }
}

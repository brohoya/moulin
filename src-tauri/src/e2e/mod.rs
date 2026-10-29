//! E2E test: one full dictation in the real app, with the sample WAV as the microphone. It never pastes.
//! It writes e2e-<time>.json and the recording e2e-<time>.wav to the output folder. Run it with "npm run e2e".
mod observer;
mod tools;
mod wer;

use crate::{
    app::{Event, Transcription, moulin},
    audio::Recording,
    chatgpt,
    report::{self, file_name},
    settings::{HotkeyMode, Settings},
    ui::overlay::View,
};
use observer::Observer;
use serde_json::{Value, json};
use std::{fs, path::PathBuf, sync::Arc, time::Duration};
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio::sync::broadcast::{Receiver, error::RecvError};

/// The recording continues after the sample. This tail is silent.
const TAIL_S: f64 = 2.5;
const MAX_WER: f64 = 0.25;
/// The states of the pop-up that the test checks.
const VIEWS: [View; 3] = [View::Recording, View::Transcribing, View::Done];

pub async fn run(app: AppHandle, page: chatgpt::Page) {
    let mut test = Test::new(app.clone(), &page);
    let observer = Observer::start(&app, &test.out_dir, &test.stamp);
    if let Err(err) = test.run(&page, &observer).await {
        test.failures.push(err);
    }
    let pass = test.finish();
    app.exit(if pass { 0 } else { 1 });
}

struct Test {
    app: AppHandle,
    settings: Settings,
    hold: bool,
    sample: PathBuf,
    /// The text of the sample: the file next to the sample, with the extension "txt".
    reference: String,
    out_dir: PathBuf,
    stamp: String,
    report: Value,
    failures: Vec<String>,
}

impl Test {
    fn new(app: AppHandle, page: &chatgpt::Page) -> Self {
        let me = moulin(&app);
        let settings = me.settings();
        let hold = settings.mode == HotkeyMode::Hold;
        let (sample, out_dir) = (me.opts.sample.clone(), me.opts.out_dir.clone());
        let reference = fs::read_to_string(sample.with_extension("txt")).unwrap_or_default().trim().to_string();
        let (test, trigger) = if hold {
            (
                "One dictation in hold mode: hold the key, release it, upload, clipboard",
                format!("the key {}, held down during the sample, then released (synthetic key events)", settings.hotkey),
            )
        } else {
            (
                "One dictation: start and stop with \"Moulin --toggle\", upload, clipboard",
                "a second Moulin process with --toggle (the path of a system shortcut)".to_string(),
            )
        };
        let started_at = report::iso_now();
        let report = json!({
            "test": test,
            "startedAt": started_at,
            "app": { "name": app.package_info().name, "version": app.package_info().version.to_string(), "packaged": !tauri::is_dev(), "executable": std::env::current_exe().ok() },
            "versions": { "tauri": tauri::VERSION, "webview": tauri::webview_version().ok(), "os": format!("{} {}", std::env::consts::OS, tools::os_version()) },
            "strategy": "B: the app records the audio (WAV, 16 kHz), and the hidden chatgpt.com page uploads it with fetch",
            "hotkey": { "accelerator": settings.hotkey, "registered": me.hotkey_error().is_none() },
            "trigger": trigger,
            "page": page,
        });
        let stamp = report::stamp(&started_at);
        Self { app, settings, hold, sample, reference, out_dir, stamp, report, failures: Vec::new() }
    }

    /// The steps of the test. An error stops the test, and the report keeps the results of the steps before it.
    async fn run(&mut self, page: &chatgpt::Page, observer: &Observer) -> Result<(), String> {
        fs::create_dir_all(&self.out_dir).map_err(|err| err.to_string())?;
        if let Some(error) = moulin(&self.app).hotkey_error() {
            self.fail(format!("The hotkey {} is not registered: {error}", self.settings.hotkey));
        }
        if !page.signed_in {
            self.fail("Not signed in to ChatGPT: open Moulin and sign in");
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
        let full_page = chatgpt::measure_full_page(&self.app, Duration::from_secs(8)).await;
        let light_page = chatgpt::memory_mb(&self.app).await;
        self.report["memoryMB"] = json!({ "app": tools::own_memory_mb(), "lightPage": light_page, "fullPage": full_page });
        let sample_s = tools::seconds(&self.sample).await?;
        self.report["sample"] = json!({ "file": file_name(&self.sample), "seconds": sample_s });
        let (recording, transcription) = self.dictate(sample_s).await?;
        let clipboard = self.app.clipboard().read_text().unwrap_or_default();
        self.check_recording(&recording, sample_s).await;
        self.check_upload(&transcription, page, &clipboard);
        self.check_overlay(observer).await;
        Ok(())
    }

    /// One dictation: the trigger starts it, the sample plays, and the trigger stops it. Returns the audio and the
    /// result of the upload.
    async fn dictate(&mut self, sample_s: f64) -> Result<(Arc<Recording>, Transcription), String> {
        let events = moulin(&self.app).events.clone();
        let mut started = events.subscribe();
        self.trigger(true).await?;
        next(&mut started, "started", 15_000, |e| matches!(e, Event::Started).then_some(())).await?;
        tokio::time::sleep(Duration::from_secs_f64(sample_s + TAIL_S)).await;
        let mut stopped = events.subscribe();
        self.trigger(false).await?;
        let audio = |e: &Event| match e {
            Event::Audio(recording) => Some(recording.clone()),
            _ => None,
        };
        let recording = next(&mut stopped, "audio", 10_000, audio).await?;
        fs::write(self.audio_file(), &recording.wav).map_err(|err| err.to_string())?;
        let result = |e: &Event| match e {
            Event::Transcription(transcription) => Some(transcription.clone()),
            _ => None,
        };
        let transcription = next(&mut stopped, "result", 300_000, result).await?;
        Ok((recording, transcription))
    }

    /// Starts or stops the dictation: the key of the hold mode, or a second Moulin process with --toggle.
    async fn trigger(&mut self, start: bool) -> Result<(), String> {
        if self.hold {
            return tools::press_key(&self.settings.hotkey, start).await;
        }
        let ms = tools::toggle_from_outside().await?;
        self.report["toggleProcessMs"][if start { "start" } else { "stop" }] = json!(ms);
        Ok(())
    }

    /// The audio: speech during the sample, and silence after its end (the sample plays one time).
    async fn check_recording(&mut self, recording: &Recording, sample_s: f64) {
        let file = self.audio_file();
        let tail_from_s = round(sample_s + 0.5, 2);
        let speech_db = tools::max_volume_db(&file, 0.0, Some(sample_s)).await;
        let tail_db = tools::max_volume_db(&file, tail_from_s, None).await;
        let silent_tail = tail_db.is_some_and(|v| v < -60.0);
        self.report["recording"] = json!({
            "file": file_name(&file),
            "mimeType": "audio/wav",
            "bytes": recording.wav.len(),
            "seconds": round(recording.duration_ms as f64 / 1000.0, 2),
            "speechMaxVolumeDb": tools::db(speech_db),
            "tailFromS": tail_from_s,
            "tailMaxVolumeDb": tools::db(tail_db),
            "noloop": silent_tail,
        });
        if !speech_db.is_some_and(|v| v > -30.0) {
            self.fail(format!("The recording has no speech ({}): the sample did not play", tools::db(speech_db)));
        }
        if !silent_tail {
            self.fail(format!("The recording after {tail_from_s} s is not silent ({})", tools::db(tail_db)));
        }
    }

    /// The upload: the HTTP status, the word error rate of the transcript, and the clipboard.
    fn check_upload(&mut self, transcription: &Transcription, page: &chatgpt::Page, clipboard: &str) {
        let outcome = &transcription.outcome;
        let ok = outcome.ok;
        let text = outcome.text.as_deref().unwrap_or_default();
        let wer = ok.then(|| wer::word_error_rate(&self.reference, text));
        self.report["request"] = json!({
            "httpStatus": outcome.status,
            "attempts": outcome.attempts,
            "page": outcome.page.or(page.kind),
            "cfMitigated": outcome.cf_mitigated,
            "error": (!ok).then_some(&outcome.message),
        });
        let stop_to_clipboard = transcription.clipboard_at.filter(|_| ok).map(|at| at.saturating_sub(transcription.stop_at));
        self.report["stopToClipboardMs"] = json!(stop_to_clipboard);
        self.report["transcript"] = json!(outcome.text);
        self.report["reference"] = json!(self.reference);
        self.report["wordErrorRate"] = json!(wer);
        self.report["clipboard"] = json!({ "matchesTranscript": ok && clipboard == text });
        if !ok {
            self.fail(format!("Upload: {}", outcome.message));
        }
        if let Some(wer) = wer.filter(|w| w.wer > MAX_WER) {
            self.fail(format!("The word error rate {} is above {MAX_WER}", wer.wer));
        }
        if ok && clipboard != text {
            self.fail("The clipboard text is not the transcript");
        }
    }

    /// The tray and the pop-up: the states, a picture of each state, and what the pop-up page shows.
    async fn check_overlay(&mut self, observer: &Observer) {
        observer.wait_for_pictures().await;
        let seen = observer.seen();
        self.report["trayStates"] = json!(seen.tray);
        self.report["overlay"] = json!(seen.overlay);
        self.report["pictures"] = Value::Object(seen.pictures);
        self.report["overlayPage"] = Value::Object(seen.pages.clone());
        if !self.settings.show_overlay {
            return;
        }
        let shown = |view: &View| seen.overlay.iter().any(|o| o.view == *view && o.visible);
        if !VIEWS.iter().all(shown) {
            self.fail("The pop-up did not show the recording, transcribing, and done states");
        }
        // The page must get each state, and the timer must run during the recording.
        let page_shows = |view: &View| seen.pages.get(view.as_str()).is_some_and(|p| p["state"] == view.as_str());
        let timer_runs = seen.pages.get("recording").and_then(|p| p["time"].as_str()).is_some_and(|t| t != "0:00");
        if !(VIEWS.iter().all(page_shows) && timer_runs) {
            self.fail(format!("The pop-up page did not show the states, or its timer did not run: {}", Value::Object(seen.pages)));
        }
    }

    /// Writes the report and prints the result. Returns true if the test passes.
    fn finish(mut self) -> bool {
        let pass = self.failures.is_empty();
        self.report["failures"] = json!(self.failures);
        self.report["pass"] = json!(pass);
        let file = self.out_dir.join(format!("e2e-{}.json", self.stamp));
        if let Err(err) = report::write(&file, &self.report) {
            eprintln!("Cannot write {}: {err}", file.display());
        }
        println!("E2E {}: {}", if pass { "PASS" } else { "FAIL" }, file.display());
        for failure in &self.failures {
            println!("- {failure}");
        }
        pass
    }

    fn fail(&mut self, failure: impl Into<String>) {
        self.failures.push(failure.into());
    }

    fn audio_file(&self) -> PathBuf {
        self.out_dir.join(format!("e2e-{}.wav", self.stamp))
    }
}

/// Waits for the next event that `pick` accepts. A microphone error stops the wait.
async fn next<T>(events: &mut Receiver<Event>, name: &str, ms: u64, pick: impl Fn(&Event) -> Option<T>) -> Result<T, String> {
    let wait = async {
        loop {
            match events.recv().await {
                Ok(Event::MicError(message)) => return Err(format!("micError: {message}")),
                Ok(event) => {
                    if let Some(value) = pick(&event) {
                        return Ok(value);
                    }
                }
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return Err("the app closed".into()),
            }
        }
    };
    tokio::time::timeout(Duration::from_millis(ms), wait).await.unwrap_or_else(|_| Err(format!("No \"{name}\" event after {ms} ms")))
}

fn round(value: f64, digits: i32) -> f64 {
    let k = 10f64.powi(digits);
    (value * k).round() / k
}

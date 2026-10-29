//! E2E test: one full dictation in the real app, with the sample WAV as the microphone. It never pastes.
//! It writes e2e-<time>.json and the recording e2e-<time>.wav to the output folder. Run it with "npm run e2e".
mod observer;
mod tools;
mod wer;

use crate::{
    app::{self, Event, Transcription, moulin},
    audio::Recording,
    chatgpt::{self, Kind, check},
    report::{self, file_name},
    settings::{HotkeyMode, Settings},
    ui::{
        self,
        overlay::{self, View},
    },
};
use observer::{Observer, page_value, save_picture};
use serde_json::{Map, Value, json};
use std::{fs, path::PathBuf, sync::Arc, time::Duration};
use tauri::{
    AppHandle, Emitter,
    async_runtime::{JoinHandle, spawn},
};
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
    /// "--challenge": the first upload gets a fake refusal of Cloudflare.
    challenge: bool,
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
        let challenge = me.opts.challenge;
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
        let test = if challenge { format!("{test}, after a fake Cloudflare refusal of the first upload") } else { test.to_string() };
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
        Self { app, settings, hold, challenge, sample, reference, out_dir, stamp, report, failures: Vec::new() }
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
        let check_pill = self.challenge.then(|| self.watch_check_pill());
        if self.challenge {
            chatgpt::fake_refusal();
        }
        let (recording, transcription) = self.dictate(sample_s).await?;
        let clipboard = self.app.clipboard().read_text().unwrap_or_default();
        self.check_recording(&recording, sample_s).await;
        self.check_upload(&transcription, page, &clipboard);
        if let Some(check_pill) = check_pill {
            self.check_cloudflare(&transcription, check_pill.await.unwrap_or(Value::Null));
            // The settings window shows the notice of a last refusal, so that its check covers the notice too.
            app::set_notice(&self.app, check::UPLOAD_NOTICE);
        }
        self.check_overlay(observer).await;
        self.check_settings_window().await;
        Ok(())
    }

    /// One dictation: the trigger starts it, the sample plays, and the trigger stops it. Returns the audio and the
    /// result of the upload.
    async fn dictate(&mut self, sample_s: f64) -> Result<(Arc<Recording>, Transcription), String> {
        let events = moulin(&self.app).events.clone();
        let mut started = events.subscribe();
        self.trigger(true).await?;
        next(&mut started, "started", 15_000, |e| matches!(e, Event::Started).then_some(())).await?;
        let end = tokio::time::Instant::now() + Duration::from_secs_f64(sample_s + TAIL_S);
        self.check_placement().await;
        // After the picture of the recording state (see observer.rs).
        tokio::time::sleep(Duration::from_millis(2500)).await;
        self.check_hover().await;
        tokio::time::sleep_until(end).await;
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

    /// The pop-up shows at the bottom center of the screen that the user works on (macOS only). A separate script finds
    /// that screen and the frame of the pop-up (see tools.rs).
    async fn check_placement(&mut self) {
        let placement = match tools::overlay_placement().await {
            Ok(placement) => placement,
            Err(err) => json!({ "error": err }),
        };
        let near = |key: &str, value: f64| placement[key].as_f64().is_some_and(|v| (v - value).abs() <= 1.0);
        let placed = placement["popupScreen"].is_string()
            && placement["popupScreen"] == placement["expectedScreen"]
            && near("offCenter", 0.0)
            && near("bottomGap", overlay::MARGIN);
        if !placement.is_null() && !placed {
            self.fail(format!("The pop-up is not at the bottom center of the screen that the user works on: {placement}"));
        }
        self.report["overlayPlacement"] = placement;
    }

    /// The pill is red only under the pointer. The test sends pointer positions to the page, as Moulin does during a
    /// recording, and reads the pill: over the pill, beside the pill in the window, and away from the window.
    /// Do not move the pointer over the pop-up during the test.
    async fn check_hover(&mut self) {
        let Some(window) = overlay::window(&self.app) else { return };
        // The pill is at the center of the window (320 by 60 points), 40 points high.
        let steps = [("overPill", Some((160.0, 30.0)), true), ("besidePill", Some((4.0, 4.0)), false), ("away", None, false)];
        let mut hover = Map::new();
        let mut follows = true;
        for (name, pointer, expected) in steps {
            let _ = self.app.emit_to(overlay::LABEL, "overlay:pointer", pointer.map(|(x, y): (f64, f64)| json!({ "x": x, "y": y })));
            tokio::time::sleep(Duration::from_millis(150)).await;
            let value = page_value(&window, "document.getElementById('pill').hasAttribute('data-hover')").await;
            follows &= value == expected;
            hover.insert(name.into(), value);
            if expected {
                hover.insert("picture".into(), save_picture(&window, &self.picture_file("pill-hover")).await);
            }
        }
        if !follows {
            self.fail(format!("The pill does not follow the pointer: {}", Value::Object(hover.clone())));
        }
        self.report["hover"] = Value::Object(hover);
    }

    /// The settings window has the height of its content, with no empty space at the bottom. The test modes do not
    /// open the window, so the test creates it, waits until its page shows it, and takes a picture.
    async fn check_settings_window(&mut self) {
        let window = match ui::settings::create(&self.app) {
            Ok(window) => window,
            Err(err) => {
                self.fail(format!("The settings window does not open: {err}"));
                return;
            }
        };
        let mut shown = false;
        for _ in 0..50 {
            shown = window.is_visible().unwrap_or(false);
            if shown {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        // The list of microphones loads after the window shows.
        tokio::time::sleep(Duration::from_millis(500)).await;
        let notice = "document.getElementById('notice').hidden ? null : document.getElementById('notice').textContent";
        let sizes = page_value(
            &window,
            &format!("({{ inner: innerHeight, content: document.body.getBoundingClientRect().height, notice: {notice} }})"),
        )
        .await;
        let picture = save_picture(&window, &self.picture_file("settings")).await;
        let _ = window.destroy();
        let fits = match (sizes["inner"].as_f64(), sizes["content"].as_f64()) {
            (Some(inner), Some(content)) => (inner - content.ceil()).abs() <= 1.0,
            _ => false,
        };
        self.report["settingsWindow"] = json!({
            "shown": shown,
            "innerHeight": sizes["inner"],
            "contentHeight": sizes["content"],
            "fits": fits,
            "notice": sizes["notice"],
            "picture": picture,
        });
        if !shown || !fits {
            self.fail(format!("The settings window does not show with the height of its content: {}", self.report["settingsWindow"]));
        }
        // --challenge: the run sets the notice of a last refusal before this check.
        if self.challenge && sizes["notice"] != check::UPLOAD_NOTICE {
            self.fail(format!("The settings window does not show the notice of the Cloudflare refusal: {}", sizes["notice"]));
        }
    }

    /// --challenge: waits until the pop-up shows the Cloudflare check. Then it reads the text of the pill, and takes a
    /// picture. The pill cuts a long text, so the test checks that the full text shows.
    fn watch_check_pill(&self) -> JoinHandle<Value> {
        let (app, file) = (self.app.clone(), self.picture_file("pill-check"));
        let mut events = moulin(&app).events.subscribe();
        spawn(async move {
            loop {
                match events.recv().await {
                    Ok(Event::Overlay { text, visible: true, .. }) if text == check::WAITING => break,
                    Ok(Event::Transcription(_)) | Err(RecvError::Closed) => return Value::Null,
                    _ => {}
                }
            }
            // The pill changes its width in 0.28 s.
            tokio::time::sleep(Duration::from_millis(350)).await;
            let Some(window) = overlay::window(&app) else { return Value::Null };
            let text = "(() => { const t = document.getElementById('text'); \
                        return { text: t.textContent, full: t.scrollWidth <= t.clientWidth }; })()";
            let mut pill = page_value(&window, text).await;
            pill["picture"] = save_picture(&window, &file).await;
            pill
        })
    }

    /// --challenge: after the fake refusal, the full page runs the check in the hidden window, and the second upload
    /// passes. The pop-up shows the full text of the check.
    fn check_cloudflare(&mut self, transcription: &Transcription, pill: Value) {
        let outcome = &transcription.outcome;
        self.report["cloudflare"] = json!({ "fakeRefusal": true, "attempts": outcome.attempts, "page": outcome.page, "pill": pill });
        if !(outcome.ok && outcome.attempts == 2 && outcome.page == Some(Kind::Full)) {
            self.fail(format!("Moulin did not pass the Cloudflare check in the hidden window: {}", self.report["cloudflare"]));
        }
        if pill["text"] != check::WAITING || pill["full"] != true {
            self.fail(format!("The pop-up did not show the full text of the Cloudflare check: {pill}"));
        }
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

    fn picture_file(&self, name: &str) -> PathBuf {
        self.out_dir.join(format!("e2e-{}-{name}.png", self.stamp))
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

// E2E test: one full dictation in the real app, with the sample WAV as the microphone. It never pastes.
// It writes e2e-<time>.json and the recording e2e-<time>.wav to the output folder. Run it with "npm run e2e".
use crate::{Event, State, chatgpt, moulin, now_ms, platform};
use serde_json::{Map, Value, json};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, async_runtime::spawn_blocking};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio::sync::broadcast::{Receiver, error::RecvError};

const TAIL_S: f64 = 2.5; // The recording continues after the sample. This tail is silent.
const MAX_WER: f64 = 0.25;

fn round(value: f64, digits: i32) -> f64 {
    let k = 10f64.powi(digits);
    (value * k).round() / k
}

/// The time in ISO 8601 (UTC), as new Date().toISOString() gives it.
pub fn iso_now() -> String {
    let ms = now_ms();
    let secs = (ms / 1000) as i64;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // The civil date of a day number (algorithm of Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z", rest / 3600, rest % 3600 / 60, rest % 60, ms % 1000)
}

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

// Words for the word error rate: lower case, no punctuation. "j'ai" gives "j" and "ai". "2.4" and "4,5" stay one word.
fn words(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.to_lowercase().chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_alphanumeric() {
            i += 1;
            continue;
        }
        let mut word = String::new();
        loop {
            while i < chars.len() && chars[i].is_alphanumeric() {
                word.push(chars[i]);
                i += 1;
            }
            if i + 1 < chars.len() && matches!(chars[i], '.' | ',' | '-') && chars[i + 1].is_alphanumeric() {
                word.push(chars[i]);
                i += 1;
            } else {
                break;
            }
        }
        out.push(word);
    }
    out
}

pub fn word_error_rate(reference: &str, transcript: &str) -> Value {
    let (refs, hyp) = (words(reference), words(transcript));
    let mut row: Vec<usize> = (0..=hyp.len()).collect();
    for i in 1..=refs.len() {
        let mut cur = vec![i; hyp.len() + 1];
        for j in 1..=hyp.len() {
            cur[j] = (row[j] + 1).min(cur[j - 1] + 1).min(row[j - 1] + usize::from(refs[i - 1] != hyp[j - 1]));
        }
        row = cur;
    }
    let edits = row[hyp.len()];
    json!({
        "wer": round(edits as f64 / refs.len().max(1) as f64, 4),
        "edits": edits,
        "referenceWords": refs.len(),
        "transcriptWords": hyp.len(),
    })
}

async fn output(command: &str, args: Vec<String>) -> Result<std::process::Output, String> {
    let name = command.to_string();
    spawn_blocking(move || Command::new(&name).args(&args).output())
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| format!("{command}: {err}"))
}

async fn seconds(file: &Path) -> Result<f64, String> {
    let args = ["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"]
        .map(String::from)
        .into_iter()
        .chain([file.display().to_string()]);
    let out = output("ffprobe", args.collect()).await?;
    String::from_utf8_lossy(&out.stdout).trim().parse().map_err(|_| format!("ffprobe cannot read {}", file.display()))
}

// The loudest level of the recording between two times, in dB. Digital silence gives -91 dB or -inf.
async fn max_volume_db(file: &Path, from_s: f64, to_s: Option<f64>) -> Option<f64> {
    let trim = match to_s {
        Some(to) => format!("atrim=start={from_s}:end={to}"),
        None => format!("atrim=start={from_s}"),
    };
    let args = vec![
        "-hide_banner".into(),
        "-nostats".into(),
        "-i".into(),
        file.display().to_string(),
        "-af".into(),
        format!("{trim},volumedetect"),
        "-f".into(),
        "null".into(),
        "-".into(),
    ];
    let out = output("ffmpeg", args).await.ok()?;
    let text = String::from_utf8_lossy(&out.stderr);
    let value = text.lines().find_map(|line| line.split("max_volume: ").nth(1))?.trim_end_matches(" dB").trim();
    if value == "-inf" { Some(f64::NEG_INFINITY) } else { value.parse().ok() }
}

fn db(value: Option<f64>) -> Value {
    match value {
        Some(v) if v.is_finite() => json!(v),
        Some(v) => json!(v.to_string()),
        None => Value::Null,
    }
}

// Hold mode (macOS): pushes or releases a function key with a synthetic key event.
// The event comes from a private event source: an event of the HID system state can get
// a key-up from macOS at the next input of the user, because no physical key is down.
async fn press_key(accelerator: &str, down: bool) -> Result<(), String> {
    const KEYCODES: [(&str, u16); 20] = [
        ("F1", 122),
        ("F2", 120),
        ("F3", 99),
        ("F4", 118),
        ("F5", 96),
        ("F6", 97),
        ("F7", 98),
        ("F8", 100),
        ("F9", 101),
        ("F10", 109),
        ("F11", 103),
        ("F12", 111),
        ("F13", 105),
        ("F14", 107),
        ("F15", 113),
        ("F16", 106),
        ("F17", 64),
        ("F18", 79),
        ("F19", 80),
        ("F20", 90),
    ];
    let keycode = KEYCODES.iter().find(|(name, _)| *name == accelerator).map(|(_, code)| *code);
    let Some(keycode) = keycode.filter(|_| cfg!(target_os = "macos")) else {
        return Err(format!("The hold test needs macOS and a shortcut from F1 to F20 (not {accelerator})"));
    };
    let script = format!(
        "ObjC.import('CoreGraphics'); const src = $.CGEventSourceCreate($.kCGEventSourceStatePrivate); \
         $.CGEventPost($.kCGHIDEventTap, $.CGEventCreateKeyboardEvent(src, {keycode}, {down}));"
    );
    let out = output("osascript", vec!["-l".into(), "JavaScript".into(), "-e".into(), script]).await?;
    if out.status.success() { Ok(()) } else { Err(format!("osascript: {}", String::from_utf8_lossy(&out.stderr).trim())) }
}

// The trigger: a second Moulin process with --toggle, the same path as a system shortcut. Returns the run time of that process.
async fn toggle_from_outside() -> Result<u64, String> {
    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    let started = Instant::now();
    let status = spawn_blocking(move || Command::new(exe).arg("--toggle").status()).await.map_err(|err| err.to_string())?;
    status.map_err(|err| err.to_string())?;
    Ok(started.elapsed().as_millis() as u64)
}

fn os_version() -> String {
    let (command, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("sw_vers", &["-productVersion"])
    } else if cfg!(windows) {
        ("cmd", &["/c", "ver"])
    } else {
        ("uname", &["-r"])
    };
    Command::new(command).args(args).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

/// What the pop-up page shows: the state of the pill, its text, and the time of the recording.
/// The time grows only with the level events of the recorder, so it also proves that they arrive.
async fn page_state(overlay: &tauri::WebviewWindow) -> Value {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx = Mutex::new(Some(tx));
    let code = "JSON.stringify({ state: document.getElementById('pill').dataset.state, \
                text: document.getElementById('text').textContent, time: document.getElementById('time').textContent })";
    let sent = overlay.eval_with_callback(code, move |json| {
        if let Some(tx) = tx.lock().unwrap().take() {
            let _ = tx.send(json);
        }
    });
    if sent.is_err() {
        return Value::Null;
    }
    match tokio::time::timeout(Duration::from_secs(2), rx).await {
        Ok(Ok(json)) => serde_json::from_str::<String>(&json).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

/// The resident memory of the Moulin process (without the WebKit processes), in MB.
fn own_memory_mb() -> Option<u64> {
    let out = Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse::<u64>().ok().map(|kb| kb / 1024)
}

pub async fn run(app: AppHandle, page: chatgpt::Page) {
    let me = moulin(&app);
    let settings = me.settings();
    let hold = settings.hold();
    let (sample, out_dir) = (me.opts.sample.clone(), me.opts.out_dir.clone());
    let started_at = iso_now();
    let stamp = started_at.replace([':', '.'], "-");
    let audio_file = out_dir.join(format!("e2e-{stamp}.wav"));
    let reference = std::fs::read_to_string(sample.with_extension("txt")).unwrap_or_default().trim().to_string();
    let mut failures: Vec<String> = Vec::new();
    let tray_states = Arc::new(Mutex::new(Vec::<State>::new()));
    let overlay_states = Arc::new(Mutex::new(Vec::<Value>::new()));
    // A picture of the pop-up in each state, for a visual check of the theme, and what the page shows at that time.
    let pictures = Arc::new(Mutex::new(Map::new()));
    let page_states = Arc::new(Mutex::new(Map::new()));
    {
        let (tray_states, overlay_states, pictures, page_states) =
            (tray_states.clone(), overlay_states.clone(), pictures.clone(), page_states.clone());
        let (mut events, app, out_dir, stamp) = (me.events.subscribe(), app.clone(), out_dir.clone(), stamp.clone());
        tauri::async_runtime::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(Event::State(state)) => tray_states.lock().unwrap().push(state),
                    Ok(Event::Overlay { view, text, visible }) => {
                        overlay_states.lock().unwrap().push(json!({ "view": view, "text": text, "visible": visible }));
                        if !visible || !["recording", "transcribing", "done"].contains(&view.as_str()) {
                            continue;
                        }
                        if pictures.lock().unwrap().insert(view.clone(), json!("pending")).is_some() {
                            continue;
                        }
                        let (pictures, page_states, app) = (pictures.clone(), page_states.clone(), app.clone());
                        let file = out_dir.join(format!("e2e-{stamp}-pill-{view}.png"));
                        tauri::async_runtime::spawn(async move {
                            // After the change of width, and with sound in the bars.
                            tokio::time::sleep(Duration::from_millis(if view == "recording" { 1500 } else { 350 })).await;
                            let taken = match app.get_webview_window("overlay") {
                                Some(overlay) => {
                                    let state = page_state(&overlay).await;
                                    page_states.lock().unwrap().insert(view.clone(), state);
                                    platform::snapshot_png(&overlay).await
                                }
                                None => Err("no pop-up".into()),
                            };
                            let value = match taken.and_then(|png| std::fs::write(&file, png).map_err(|err| err.to_string())) {
                                Ok(()) => json!(file.file_name().map(|n| n.to_string_lossy().into_owned())),
                                Err(err) => json!(format!("not taken: {err}")),
                            };
                            pictures.lock().unwrap().insert(view, value);
                        });
                    }
                    Err(RecvError::Closed) => return,
                    _ => {}
                }
            }
        });
    }
    let mut report = json!({
        "test": if hold { "One dictation in hold mode: hold the key, release it, upload, clipboard" } else { "One dictation: start and stop with \"Moulin --toggle\", upload, clipboard" },
        "startedAt": started_at,
        "app": { "name": app.package_info().name, "version": app.package_info().version.to_string(), "packaged": !tauri::is_dev(), "executable": std::env::current_exe().ok() },
        "versions": { "tauri": tauri::VERSION, "webview": tauri::webview_version().ok(), "os": format!("{} {}", std::env::consts::OS, os_version()) },
        "strategy": "B: the app records the audio (WAV, 16 kHz), and the hidden chatgpt.com page uploads it with fetch",
        "hotkey": { "accelerator": settings.hotkey, "registered": me.hotkey_error().is_none() },
        "trigger": if hold { format!("the key {}, held down during the sample, then released (synthetic key events)", settings.hotkey) } else { "a second Moulin process with --toggle (the path of a system shortcut)".into() },
        "page": page,
    });
    let outcome: Result<(), String> = async {
        std::fs::create_dir_all(&out_dir).map_err(|err| err.to_string())?;
        if let Some(error) = me.hotkey_error() {
            failures.push(format!("The hotkey {} is not registered: {error}", settings.hotkey));
        }
        if !page.signed_in {
            failures.push("Not signed in to ChatGPT: open Moulin and sign in".into());
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
        let full = chatgpt::measure_full_page(&app, Duration::from_secs(8)).await;
        report["memoryMB"] = json!({ "app": own_memory_mb(), "lightPage": chatgpt::memory_mb(&app).await, "fullPage": full["fullPage"] });
        let sample_s = seconds(&sample).await?;
        report["sample"] = json!({ "file": sample.file_name().map(|n| n.to_string_lossy().into_owned()), "seconds": sample_s });

        let mut events = me.events.subscribe();
        if hold {
            press_key(&settings.hotkey, true).await?;
        } else {
            report["toggleProcessMs"] = json!({ "start": toggle_from_outside().await? });
        }
        next(&mut events, "started", 15_000, |e| matches!(e, Event::Started).then_some(())).await?;
        tokio::time::sleep(Duration::from_secs_f64(sample_s + TAIL_S)).await;
        let mut events = me.events.subscribe();
        if hold {
            press_key(&settings.hotkey, false).await?;
        } else {
            report["toggleProcessMs"]["stop"] = json!(toggle_from_outside().await?);
        }
        let pick_audio = |e: &Event| match e {
            Event::Audio { wav, duration_ms } => Some((wav.clone(), *duration_ms)),
            _ => None,
        };
        let (wav, duration_ms) = next(&mut events, "audio", 10_000, pick_audio).await?;
        std::fs::write(&audio_file, wav.as_slice()).map_err(|err| err.to_string())?;
        let pick_result = |e: &Event| match e {
            Event::Result(value) => Some(value.clone()),
            _ => None,
        };
        let result = next(&mut events, "result", 300_000, pick_result).await?;
        let clipboard_text = app.clipboard().read_text().unwrap_or_default();

        let ok = result["ok"].as_bool().unwrap_or(false);
        let text = result["text"].as_str().unwrap_or_default().to_string();
        let tail_from_s = round(sample_s + 0.5, 2);
        let speech_db = max_volume_db(&audio_file, 0.0, Some(sample_s)).await;
        let tail_db = max_volume_db(&audio_file, tail_from_s, None).await;
        let noloop = tail_db.is_some_and(|v| v < -60.0);
        report["recording"] = json!({
            "file": audio_file.file_name().map(|n| n.to_string_lossy().into_owned()),
            "mimeType": "audio/wav",
            "bytes": wav.len(),
            "seconds": round(duration_ms as f64 / 1000.0, 2),
            "speechMaxVolumeDb": db(speech_db),
            "tailFromS": tail_from_s,
            "tailMaxVolumeDb": db(tail_db),
            "noloop": noloop,
        });
        if !speech_db.is_some_and(|v| v > -30.0) {
            failures.push(format!("The recording has no speech ({}): the sample did not play", db(speech_db)));
        }
        report["request"] = json!({
            "httpStatus": result["status"],
            "attempts": result["attempts"],
            "page": if result["page"].is_null() { json!(page.kind) } else { result["page"].clone() },
            "cfMitigated": result["cfMitigated"],
            "error": if ok { Value::Null } else { result["message"].clone() },
        });
        report["stopToClipboardMs"] = match (ok, result["clipboardAt"].as_u64(), result["stopAt"].as_u64()) {
            (true, Some(at), Some(stop)) => json!(at.saturating_sub(stop)),
            _ => Value::Null,
        };
        report["transcript"] = result["text"].clone();
        report["reference"] = json!(reference);
        report["wordErrorRate"] = if ok { word_error_rate(&reference, &text) } else { Value::Null };
        report["clipboard"] = json!({ "matchesTranscript": ok && clipboard_text == text });
        report["trayStates"] = json!(*tray_states.lock().unwrap());
        report["overlay"] = json!(*overlay_states.lock().unwrap());
        for _ in 0..30 {
            if !pictures.lock().unwrap().values().any(|v| v == "pending") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        report["pictures"] = Value::Object(pictures.lock().unwrap().clone());
        report["overlayPage"] = Value::Object(page_states.lock().unwrap().clone());
        let shown = |view: &str| overlay_states.lock().unwrap().iter().any(|o| o["view"] == view && o["visible"] == true);
        if settings.show_overlay && !["recording", "transcribing", "done"].iter().all(|v| shown(v)) {
            failures.push("The pop-up did not show the recording, transcribing, and done states".into());
        }
        // The page must get each state, and the timer must run during the recording.
        let pages = page_states.lock().unwrap().clone();
        let page_shows = |view: &str| pages.get(view).is_some_and(|p| p["state"] == view);
        let timer_runs = pages.get("recording").and_then(|p| p["time"].as_str()).is_some_and(|t| t != "0:00");
        if settings.show_overlay && !(["recording", "transcribing", "done"].iter().all(|v| page_shows(v)) && timer_runs) {
            failures.push(format!("The pop-up page did not show the states, or its timer did not run: {}", Value::Object(pages)));
        }
        if !noloop {
            failures.push(format!("The recording after {tail_from_s} s is not silent ({})", db(tail_db)));
        }
        if !ok {
            failures.push(format!("Upload: {}", result["message"].as_str().unwrap_or_default()));
        }
        if ok && report["wordErrorRate"]["wer"].as_f64().is_some_and(|wer| wer > MAX_WER) {
            failures.push(format!("The word error rate {} is above {MAX_WER}", report["wordErrorRate"]["wer"]));
        }
        if ok && clipboard_text != text {
            failures.push("The clipboard text is not the transcript".into());
        }
        Ok(())
    }
    .await;
    if let Err(err) = outcome {
        failures.push(err);
    }
    let pass = failures.is_empty();
    report["failures"] = json!(failures);
    report["pass"] = json!(pass);
    let file: PathBuf = out_dir.join(format!("e2e-{stamp}.json"));
    let written = serde_json::to_string_pretty(&report).map(|text| std::fs::write(&file, format!("{text}\n")));
    if !matches!(written, Ok(Ok(()))) {
        eprintln!("Cannot write {}", file.display());
    }
    println!("E2E {}: {}", if pass { "PASS" } else { "FAIL" }, file.display());
    for failure in &failures {
        println!("- {failure}");
    }
    app.exit(if pass { 0 } else { 1 });
}

//! The programs that the test runs: ffprobe and ffmpeg for the audio, osascript for the key of the hold mode,
//! a second Moulin for the toggle, and ps and sw_vers for the facts of the report.
use serde_json::{Value, json};
use std::{
    path::Path,
    process::{Command, Output},
    time::Instant,
};
use tauri::async_runtime::spawn_blocking;

async fn output(command: &str, args: Vec<String>) -> Result<Output, String> {
    let name = command.to_string();
    spawn_blocking(move || Command::new(&name).args(&args).output())
        .await
        .map_err(|err| err.to_string())?
        .map_err(|err| format!("{command}: {err}"))
}

/// The length of an audio file, in seconds.
pub async fn seconds(file: &Path) -> Result<f64, String> {
    let args = ["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"]
        .map(String::from)
        .into_iter()
        .chain([file.display().to_string()]);
    let out = output("ffprobe", args.collect()).await?;
    String::from_utf8_lossy(&out.stdout).trim().parse().map_err(|_| format!("ffprobe cannot read {}", file.display()))
}

/// The loudest level of the recording between two times, in dB. Digital silence gives -91 dB or -inf.
pub async fn max_volume_db(file: &Path, from_s: f64, to_s: Option<f64>) -> Option<f64> {
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

/// A level in dB for the report. JSON has no infinity, so -inf becomes a string.
pub fn db(value: Option<f64>) -> Value {
    match value {
        Some(v) if v.is_finite() => json!(v),
        Some(v) => json!(v.to_string()),
        None => Value::Null,
    }
}

/// Hold mode (macOS): pushes or releases a function key with a synthetic key event.
/// The event comes from a private event source: an event of the HID system state can get
/// a key-up from macOS at the next input of the user, because no physical key is down.
pub async fn press_key(accelerator: &str, down: bool) -> Result<(), String> {
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

/// The trigger of the toggle mode: a second Moulin process with --toggle, the same path as a system shortcut.
/// Returns the run time of that process, in ms.
pub async fn toggle_from_outside() -> Result<u64, String> {
    let exe = std::env::current_exe().map_err(|err| err.to_string())?;
    let started = Instant::now();
    let status = spawn_blocking(move || Command::new(exe).arg("--toggle").status()).await.map_err(|err| err.to_string())?;
    status.map_err(|err| err.to_string())?;
    Ok(started.elapsed().as_millis() as u64)
}

pub fn os_version() -> String {
    let (command, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("sw_vers", &["-productVersion"])
    } else if cfg!(windows) {
        ("cmd", &["/c", "ver"])
    } else {
        ("uname", &["-r"])
    };
    Command::new(command).args(args).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

/// The script that finds the screen of the pop-up (see placement.js).
const PLACEMENT: &str = include_str!("placement.js");

/// The placement of the pop-up (see PLACEMENT). Null on Windows and Linux.
pub async fn overlay_placement() -> Result<Value, String> {
    if !cfg!(target_os = "macos") {
        return Ok(Value::Null);
    }
    let args = vec!["-l".into(), "JavaScript".into(), "-e".into(), PLACEMENT.into(), std::process::id().to_string()];
    let out = output("osascript", args).await?;
    if !out.status.success() {
        return Err(format!("osascript: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    serde_json::from_slice(&out.stdout).map_err(|err| format!("osascript: {err}"))
}

/// The resident memory of the Moulin process (without the WebKit processes), in MB.
pub fn own_memory_mb() -> Option<u64> {
    let out = Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse::<u64>().ok().map(|kb| kb / 1024)
}

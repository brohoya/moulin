//! "--media=lower": the volume of the computer during a dictation. The test sets the volume to 40 %, and a watcher (a
//! program of the system, not the code of Moulin) reports each change of the volume. During the recording, the volume
//! must be the percent of the setting, and after the stop, 40 % again. After a quick tap, the volume must also be 40 %.
//! Last, "the user" sets the volume to 60 % during a recording: Moulin must keep it after the recording. At the end,
//! the test sets back the volume that the user had before the test.
use crate::app::now_ms;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

/// The volume at the start of the test, from 0 to 1.
const START: f64 = 0.4;
/// The volume that "the user" sets during a recording.
const USER: f64 = 0.6;
/// The maximum time from a trigger of the test to the change of the volume.
const LIMIT_MS: u64 = 3000;
/// A difference of volume that a system can make when it keeps a value.
const CLOSE: f64 = 0.03;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= CLOSE
}

/// The volumes that the watcher reported: the time in ms since 1970, and the volume from 0 to 1.
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<(u64, f64)>>>);

impl Log {
    fn push(&self, at: u64, volume: f64) {
        self.lock().push((at, volume));
    }

    fn lock(&self) -> MutexGuard<'_, Vec<(u64, f64)>> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The volume at a time: the last volume that the watcher reported before it.
    fn at(&self, at: u64) -> Option<f64> {
        self.lock().iter().rev().find(|(t, _)| *t <= at).map(|(_, volume)| *volume)
    }

    /// The first time after `after` when the watcher reported a volume close to `volume`.
    fn first_after(&self, after: u64, volume: f64) -> Option<u64> {
        self.lock().iter().find(|(t, v)| *t > after && close(*v, volume)).map(|(t, _)| *t)
    }

    /// The volumes between two times.
    fn between(&self, from: u64, to: u64) -> Vec<f64> {
        self.lock().iter().filter(|(t, _)| *t > from && *t < to).map(|(_, volume)| *volume).collect()
    }

    /// The volume at `origin`, then each change after it, with the time from `origin`, for the report.
    fn timeline(&self, origin: u64) -> Value {
        let log = self.lock();
        let first = log.iter().rposition(|(t, _)| *t <= origin).unwrap_or(0);
        log[first..].iter().map(|(t, volume)| json!({ "ms": *t as i64 - origin as i64, "volume": volume })).collect()
    }
}

pub struct Watch {
    log: Log,
    /// The volume of the user before the test.
    before: f64,
    _watcher: system::Handle,
}

impl Watch {
    /// Keeps the volume of the user, sets the volume to `START`, and starts the watcher.
    pub async fn start() -> Result<Self, String> {
        let before = system::get()?;
        system::set(START)?;
        let log = Log::default();
        let watch = Self { log: log.clone(), before, _watcher: system::watch(log)? };
        for _ in 0..150 {
            if watch.log.at(u64::MAX).is_some_and(|volume| close(volume, START)) {
                return Ok(watch);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err(format!("The volume watcher did not report {START}: {}", watch.log.timeline(0)))
    }

    /// Checks the dictation. `start_at` and `stop_at` are the times of the triggers, and `percent` is the setting.
    /// Returns the report and the failures.
    pub async fn check(&self, start_at: u64, stop_at: u64, percent: u8) -> (Value, Vec<String>) {
        let low = START * f64::from(percent) / 100.0;
        while self.log.first_after(stop_at, START).is_none() && now_ms() < stop_at + LIMIT_MS {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let lowered_at = self.log.first_after(start_at, low);
        let back_at = self.log.first_after(stop_at, START);
        let mut failures = Vec::new();
        if !self.log.at(start_at).is_some_and(|volume| close(volume, START)) {
            failures.push(format!("Volume: the volume was not {START} before the dictation"));
        }
        // A setting close to 100 % changes nothing.
        if !close(low, START) {
            match lowered_at {
                Some(at) if at - start_at <= LIMIT_MS && at < stop_at => {}
                _ => failures.push(format!("Volume: Moulin did not lower the volume to {percent} %, in {LIMIT_MS} ms after the start")),
            }
            if lowered_at.is_some_and(|at| self.log.between(at, stop_at).iter().any(|volume| !close(*volume, low))) {
                failures.push("Volume: the volume did not stay low until the stop".to_string());
            }
            if !back_at.is_some_and(|at| at - stop_at <= LIMIT_MS) {
                failures.push(format!("Volume: Moulin did not set the volume back to {START}, in {LIMIT_MS} ms after the stop"));
            }
        }
        let report = json!({
            "start": START,
            "percent": percent,
            "low": low,
            "timeline": self.log.timeline(start_at),
            "loweredAfterStartMs": lowered_at.map(|at| at - start_at),
            "backAfterStopMs": back_at.map(|at| at - stop_at),
        });
        (report, failures)
    }

    /// Checks a quick tap: a start and a cancel at once. After the cancel, the volume must be `START` again. The
    /// watcher reads the volume each 20 to 100 ms, so it can miss a short change. Returns the report and the failures.
    pub async fn check_tap(&self, start_at: u64, cancel_at: u64) -> (Value, Vec<String>) {
        tokio::time::sleep(Duration::from_millis(LIMIT_MS)).await;
        let end = self.log.at(u64::MAX);
        let mut failures = Vec::new();
        if !end.is_some_and(|volume| close(volume, START)) {
            failures.push(format!("Volume, quick tap: the volume is {end:?} after the cancel, not {START}"));
        }
        (json!({ "timeline": self.log.timeline(start_at), "cancelFromStartMs": cancel_at - start_at }), failures)
    }

    /// "The user" sets the volume to `USER` during the recording, after Moulin lowered it to `percent`.
    pub async fn change_during_recording(&self, start_at: u64, percent: u8) {
        let low = START * f64::from(percent) / 100.0;
        for _ in 0..30 {
            if self.log.first_after(start_at, low).is_some() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if let Err(err) = system::set(USER) {
            return eprintln!("The test cannot set the volume: {err}");
        }
        for _ in 0..30 {
            if self.log.at(u64::MAX).is_some_and(|volume| close(volume, USER)) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Checks a change by the user during a recording: after the cancel, the volume must stay `USER`. Returns the
    /// report and the failures.
    pub async fn check_user(&self, start_at: u64, cancel_at: u64) -> (Value, Vec<String>) {
        tokio::time::sleep(Duration::from_millis(LIMIT_MS)).await;
        let end = self.log.at(u64::MAX);
        let mut failures = Vec::new();
        if self.log.first_after(start_at, USER).is_none() {
            failures.push(format!("Volume, user change: the test could not set the volume to {USER}"));
        } else if !end.is_some_and(|volume| close(volume, USER)) {
            failures.push(format!("Volume, user change: Moulin did not keep the volume of the user ({USER}): {end:?}"));
        }
        (json!({ "user": USER, "timeline": self.log.timeline(start_at), "cancelFromStartMs": cancel_at - start_at }), failures)
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        if let Err(err) = system::set(self.before) {
            eprintln!("The test cannot set back the volume of the user ({}): {err}", self.before);
        }
    }
}

/// Windows: Core Audio through C# in PowerShell (see volume.ps1).
#[cfg(windows)]
mod system {
    use super::Log;
    use base64::Engine;
    use std::{
        io::{BufRead, BufReader},
        os::windows::process::CommandExt,
        process::{Child, Command, Stdio},
        thread,
    };

    const SCRIPT: &str = include_str!("volume.ps1");
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// The watcher process. It stops when the test drops the handle.
    pub struct Handle(Child);

    impl Drop for Handle {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn powershell(action: &str, value: f64) -> Command {
        let script = format!("$Action = '{action}'; $Value = '{value}'\n{SCRIPT}");
        let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut command = Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &base64::engine::general_purpose::STANDARD.encode(utf16)])
            .stdin(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW);
        command
    }

    /// Runs an action, and returns its output.
    fn run(action: &str, value: f64) -> Result<String, String> {
        let out = powershell(action, value).output().map_err(|err| format!("powershell.exe: {err}"))?;
        if !out.status.success() {
            return Err(format!("the volume script ({action}) failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    pub fn get() -> Result<f64, String> {
        let text = run("get", 0.0)?;
        text.trim().parse().map_err(|_| format!("the volume script gave no volume: {}", text.trim()))
    }

    pub fn set(volume: f64) -> Result<(), String> {
        run("set", volume).map(drop)
    }

    pub fn watch(log: Log) -> Result<Handle, String> {
        let mut child = powershell("watch", 0.0)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| format!("powershell.exe: {err}"))?;
        let stdout = child.stdout.take().ok_or("no output of the volume watcher")?;
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some((at, volume)) = line.trim().split_once(' ')
                    && let (Ok(at), Ok(volume)) = (at.parse(), volume.parse())
                {
                    log.push(at, volume);
                }
            }
        });
        Ok(Handle(child))
    }
}

/// Linux: pactl, on the default sink.
#[cfg(target_os = "linux")]
mod system {
    use std::process::Command;

    pub use super::poll::{Handle, watch};

    fn pactl(args: &[&str]) -> Result<String, String> {
        let out = Command::new("pactl").args(args).env("LC_ALL", "C").output().map_err(|err| format!("pactl: {err}"))?;
        if !out.status.success() {
            return Err(format!("pactl: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// The volume of the first channel. pactl writes "Volume: front-left: 26214 /  40% / -23.88 dB, ...".
    pub fn get() -> Result<f64, String> {
        let text = pactl(&["get-sink-volume", "@DEFAULT_SINK@"])?;
        let percent = text.split('/').nth(1).and_then(|part| part.trim().strip_suffix('%')?.trim().parse::<f64>().ok());
        percent.map(|percent| percent / 100.0).ok_or_else(|| format!("pactl: unknown volume: {}", text.trim()))
    }

    pub fn set(volume: f64) -> Result<(), String> {
        pactl(&["set-sink-volume", "@DEFAULT_SINK@", &format!("{}%", (volume * 100.0).round())]).map(drop)
    }
}

/// macOS: osascript, on the output volume of the system.
#[cfg(target_os = "macos")]
mod system {
    use std::process::Command;

    pub use super::poll::{Handle, watch};

    fn osascript(script: &str) -> Result<String, String> {
        let out = Command::new("/usr/bin/osascript").args(["-e", script]).output().map_err(|err| format!("osascript: {err}"))?;
        if !out.status.success() {
            return Err(format!("osascript: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    pub fn get() -> Result<f64, String> {
        let text = osascript("output volume of (get volume settings)")?;
        text.trim().parse::<f64>().map(|volume| volume / 100.0).map_err(|_| format!("osascript: no volume: {}", text.trim()))
    }

    pub fn set(volume: f64) -> Result<(), String> {
        osascript(&format!("set volume output volume {}", (volume * 100.0).round())).map(drop)
    }
}

/// Linux and macOS: a thread reads the volume with `system::get` each 50 ms, and reports each change.
#[cfg(not(windows))]
mod poll {
    use super::{Log, system};
    use crate::app::now_ms;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::Duration,
    };

    /// The thread stops when the test drops the handle.
    pub struct Handle(Arc<AtomicBool>);

    impl Drop for Handle {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    pub fn watch(log: Log) -> Result<Handle, String> {
        system::get()?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        thread::spawn(move || {
            let mut last = None;
            while !stopped.load(Ordering::Relaxed) {
                if let Ok(volume) = system::get()
                    && last != Some(volume)
                {
                    log.push(now_ms(), volume);
                    last = Some(volume);
                }
                thread::sleep(Duration::from_millis(50));
            }
        });
        Ok(Handle(stop))
    }
}

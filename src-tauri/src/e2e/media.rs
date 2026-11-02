//! "--media": two test players check the pause of the media. Before the dictation, the player "a" plays, and the
//! player "b" is paused. During the recording, "a" must be paused. After the stop, "a" must play again, and "b" must
//! stay paused, because Moulin never starts media that was not playing. Then a quick tap (a start and a cancel at once)
//! must leave "a" playing. Last, "a" quits, so that "b" is the only player, and it is paused: after one more quick tap,
//! "b" must stay paused. On macOS, only this last step can show a play to an app that did not play, because Now Playing
//! gives the commands to one app, "a" before it quits.
//!
//! The players use the media interface of the system, like a music app: a WinRT MediaPlayer in a PowerShell process on
//! Windows, an MPRIS player on the D-Bus session bus on Linux, and a small Now Playing app on macOS. Each player writes
//! the time of each change of its state, so the test sees the states without the media code of Moulin.
use crate::app::now_ms;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

/// The players, and their state before the dictation. "b" starts first: on macOS, the Now Playing app is the app that
/// started to play last, and it must be "a".
const PLAYERS: [(&str, &str); 2] = [("b", "paused"), ("a", "playing")];
/// The maximum time from a trigger of the test to the change of the player.
const LIMIT_MS: u64 = 3000;
/// The Linux and macOS players apply a command after this time, like a real app, so that a resume right after a pause
/// must wait for the pause. It is longer than the 0.4 s that the macOS script waits after a command, so that the resume
/// script waits too. The Windows player applies a command at once (see the Windows player).
#[cfg(not(windows))]
const DELAY: Duration = Duration::from_millis(600);

/// The states that a player reported: the time in ms since 1970, and the state ("playing", "paused", ...).
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<(u64, String)>>>);

impl Log {
    fn push_at(&self, at: u64, state: &str) {
        self.lock().push((at, state.to_string()));
    }

    fn lock(&self) -> MutexGuard<'_, Vec<(u64, String)>> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The state at a time: the last state that the player reported before it.
    fn state_at(&self, at: u64) -> Option<String> {
        self.lock().iter().rev().find(|(t, _)| *t <= at).map(|(_, state)| state.clone())
    }

    /// The first time after `after` when the player reported `state`.
    fn first_after(&self, after: u64, state: &str) -> Option<u64> {
        self.lock().iter().find(|(t, s)| *t > after && s == state).map(|(t, _)| *t)
    }

    /// The reports between two times, for example a play during the recording.
    fn between(&self, from: u64, to: u64) -> Vec<String> {
        self.lock().iter().filter(|(t, _)| *t > from && *t < to).map(|(_, state)| state.clone()).collect()
    }

    /// The states with their time from `origin`, for the report.
    fn timeline(&self, origin: u64) -> Value {
        self.lock().iter().map(|(t, state)| json!({ "ms": *t as i64 - origin as i64, "state": state })).collect()
    }
}

struct Player {
    name: &'static str,
    before: &'static str,
    log: Log,
    /// The process or the connection of the player. The player quits when the test drops it.
    handle: Option<system::Handle>,
}

pub struct Players {
    players: Vec<Player>,
    /// The temporary folder of the test files: the silent WAV file, and the app of the macOS players.
    folder: PathBuf,
}

impl Players {
    /// Starts the players one after the other. Each one must have its state before the next one starts. Then the system
    /// gets a moment to show them to other apps.
    pub async fn start() -> Result<Self, String> {
        let folder = std::env::temp_dir().join(format!("moulin-e2e-media-{}", std::process::id()));
        fs::create_dir_all(&folder).map_err(|err| err.to_string())?;
        let mut players = Self { players: Vec::new(), folder };
        let silence = players.folder.join("silence.wav");
        fs::write(&silence, crate::audio::wav::encode(&[0; 16_000], 16_000)).map_err(|err| err.to_string())?;
        for (name, before) in PLAYERS {
            let log = Log::default();
            let handle = system::start(name, before, log.clone(), &silence)?;
            players.players.push(Player { name, before, log: log.clone(), handle: Some(handle) });
            let mut ready = false;
            for _ in 0..150 {
                ready = log.state_at(u64::MAX).as_deref() == Some(before);
                if ready {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            if !ready {
                return Err(format!("The test player \"{name}\" did not start: {}", players.timelines(now_ms())));
            }
        }
        tokio::time::sleep(Duration::from_millis(1500)).await;
        Ok(players)
    }

    /// Checks what the players reported. `start_at` and `stop_at` are the times of the triggers of the dictation.
    /// Returns the report and the failures.
    pub async fn check(&self, start_at: u64, stop_at: u64) -> (Value, Vec<String>) {
        let (a, b) = (self.log("a"), self.log("b"));
        // The play after the stop can take a moment. Then a wrong play of "b" has the time to show.
        while a.first_after(stop_at, "playing").is_none() && now_ms() < stop_at + LIMIT_MS {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let paused_at = a.first_after(start_at, "paused");
        let playing_at = a.first_after(stop_at, "playing");
        let mut failures = Vec::new();
        if a.state_at(start_at).as_deref() != Some("playing") || b.state_at(start_at).as_deref() != Some("paused") {
            failures.push("Media: the test players did not have their states before the dictation".to_string());
        }
        match paused_at {
            Some(at) if at - start_at <= LIMIT_MS && at < stop_at => {}
            _ => failures.push(format!("Media: Moulin did not pause the player that played, in {LIMIT_MS} ms after the start")),
        }
        if paused_at.is_some_and(|at| a.between(at, stop_at).iter().any(|state| state != "paused")) {
            failures.push("Media: the player did not stay paused until the stop".to_string());
        }
        if !playing_at.is_some_and(|at| at - stop_at <= LIMIT_MS) {
            failures.push(format!("Media: Moulin did not play the player again, in {LIMIT_MS} ms after the stop"));
        }
        // "b" was paused before the dictation: Moulin must not start it, and must not send it a command.
        let b_changes = b.between(start_at, u64::MAX);
        if !b_changes.is_empty() {
            failures.push(format!("Media: Moulin changed the player that was paused before the dictation: {b_changes:?}"));
        }
        let report = json!({
            "interface": system::INTERFACE,
            "players": self.timelines(start_at),
            "pausedAfterStartMs": paused_at.map(|at| at - start_at),
            "playingAfterStopMs": playing_at.map(|at| at - stop_at),
            "stopFromStartMs": stop_at - start_at,
        });
        (report, failures)
    }

    /// Checks a quick tap: a start and a cancel at once. The pause can come after the cancel, so a resume that does not
    /// wait for it leaves the media paused. At the end, "a" must play, and "b" must not change. The Windows player reads
    /// its state each 20 ms, so it can miss a shorter pause: the report tells if the pause was seen. The Linux and macOS
    /// players report each command, so there the pause must show. Returns the report and the failures.
    pub async fn check_tap(&self, start_at: u64, cancel_at: u64) -> (Value, Vec<String>) {
        let (a, b) = (self.log("a"), self.log("b"));
        let paused_at = || a.first_after(start_at, "paused");
        let playing_at = || paused_at().and_then(|paused| a.first_after(paused, "playing"));
        // The play after a seen pause ends the wait. Without a seen pause, the wait goes to the limit.
        while playing_at().is_none() && now_ms() < cancel_at + LIMIT_MS {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let mut failures = Vec::new();
        if cfg!(not(windows)) && paused_at().is_none() {
            failures.push("Media, quick tap: Moulin did not pause the player that played".to_string());
        }
        if a.state_at(u64::MAX).as_deref() != Some("playing") {
            failures.push(format!("Media, quick tap: the player that played does not play {LIMIT_MS} ms after the cancel"));
        }
        let b_changes = b.between(start_at, u64::MAX);
        if !b_changes.is_empty() {
            failures.push(format!("Media, quick tap: Moulin changed the player that was paused: {b_changes:?}"));
        }
        let report = json!({
            "players": self.timelines(start_at),
            "cancelFromStartMs": cancel_at - start_at,
            "pauseSeen": paused_at().is_some(),
            "pausedAfterStartMs": paused_at().map(|at| at - start_at),
            "playingAfterCancelMs": playing_at().map(|at| at as i64 - cancel_at as i64),
        });
        (report, failures)
    }

    /// Stops a player: its process quits, or its connection closes. Its log stays for the report.
    pub fn quit(&mut self, name: &str) {
        if let Some(player) = self.players.iter_mut().find(|p| p.name == name) {
            player.handle = None;
        }
    }

    /// Checks a quick tap while nothing plays: "b" is paused, and it must not change. Returns the report and the failures.
    pub async fn check_idle(&self, start_at: u64, cancel_at: u64) -> (Value, Vec<String>) {
        tokio::time::sleep(Duration::from_millis(LIMIT_MS)).await;
        let changes = self.log("b").between(start_at, u64::MAX);
        let mut failures = Vec::new();
        if !changes.is_empty() {
            failures.push(format!("Media, nothing plays: Moulin changed the paused player: {changes:?}"));
        }
        let report = json!({ "players": self.timelines(start_at), "cancelFromStartMs": cancel_at - start_at });
        (report, failures)
    }

    fn log(&self, name: &str) -> &Log {
        &self.players.iter().find(|p| p.name == name).expect("a test player").log
    }

    /// The reports of each player, with their time from `origin`.
    fn timelines(&self, origin: u64) -> Value {
        self.players.iter().map(|p| (p.name.to_string(), json!({ "before": p.before, "timeline": p.log.timeline(origin) }))).collect()
    }
}

impl Drop for Players {
    fn drop(&mut self) {
        self.players.clear(); // The players stop first, so that their files are free.
        let _ = fs::remove_dir_all(&self.folder);
    }
}

/// Reads the lines "<ms> <state>" of a player process into its log.
#[cfg(not(target_os = "linux"))]
fn follow(stdout: std::process::ChildStdout, log: Log) {
    use std::io::{BufRead, BufReader};
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some((at, state)) = line.trim().split_once(' ')
                && let Ok(at) = at.parse()
            {
                log.push_at(at, state);
            }
        }
    });
}

/// Windows: a WinRT MediaPlayer in a PowerShell process (see player.ps1). Each process is a separate media session. The
/// MediaPlayer applies a command at once, and Windows PowerShell cannot get the events of WinRT, so this player cannot
/// apply a command late. Windows shows the new state of a session about 20 ms after the command.
#[cfg(windows)]
mod system {
    use super::{Log, Path};
    use base64::Engine;
    use std::{
        os::windows::process::CommandExt,
        process::{Child, Command, Stdio},
    };

    pub const INTERFACE: &str = "Windows media sessions (WinRT MediaPlayer in PowerShell)";
    const SCRIPT: &str = include_str!("player.ps1");
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub struct Handle(Child);

    impl Drop for Handle {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    pub fn start(_name: &str, before: &str, log: Log, silence: &Path) -> Result<Handle, String> {
        let uri = tauri::Url::from_file_path(silence).map_err(|_| format!("no URI for {}", silence.display()))?;
        let script = format!("$Wav = '{}'; $Start = '{before}'\n{SCRIPT}", uri.as_str().replace('\'', "''"));
        let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
        // An encoded command needs no change of the execution policy. The script writes its errors to its output, so
        // the error output has only the "#< CLIXML" line of PowerShell.
        let mut child = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &base64::engine::general_purpose::STANDARD.encode(utf16)])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|err| format!("powershell.exe: {err}"))?;
        super::follow(child.stdout.take().ok_or("no output of powershell.exe")?, log);
        Ok(Handle(child))
    }
}

/// Linux: an MPRIS player on the session bus. Each player has its own connection, like a separate app. The player
/// applies a command late, and reports each command, also one that does not change its state.
#[cfg(target_os = "linux")]
mod system {
    use super::{DELAY, Log, Path};
    use crate::app::now_ms;
    use std::sync::{Arc, Mutex};
    use zbus::blocking::{Connection, connection};

    pub const INTERFACE: &str = "MPRIS on the D-Bus session bus";

    /// The player stays on the bus until the test drops its connection.
    pub struct Handle {
        _bus: Connection,
    }

    struct Player {
        state: Arc<Mutex<&'static str>>,
        log: Log,
    }

    impl Player {
        fn state(&self) -> &'static str {
            *self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
        }

        /// Applies a command after `DELAY`, and reports it then, also when the state does not change.
        fn set(&self, next: &'static str) {
            let (state, log) = (self.state.clone(), self.log.clone());
            std::thread::spawn(move || {
                std::thread::sleep(DELAY);
                *state.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = next;
                log.push_at(now_ms(), next);
            });
        }
    }

    #[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
    impl Player {
        fn play(&self) {
            self.set("playing");
        }

        fn pause(&self) {
            self.set("paused");
        }

        fn play_pause(&self) {
            self.set(if self.state() == "playing" { "paused" } else { "playing" });
        }

        fn stop(&self) {
            self.set("stopped");
        }

        #[zbus(property)]
        fn playback_status(&self) -> String {
            match self.state() {
                "playing" => "Playing",
                "paused" => "Paused",
                _ => "Stopped",
            }
            .into()
        }

        #[zbus(property)]
        fn can_play(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn can_pause(&self) -> bool {
            true
        }

        #[zbus(property)]
        fn can_control(&self) -> bool {
            true
        }
    }

    pub fn start(name: &str, before: &'static str, log: Log, _silence: &Path) -> Result<Handle, String> {
        log.push_at(now_ms(), before);
        let player = Player { state: Arc::new(Mutex::new(before)), log };
        connection::Builder::session()
            .and_then(|builder| {
                builder.name(format!("org.mpris.MediaPlayer2.moulin_e2e_{name}"))?.serve_at("/org/mpris/MediaPlayer2", player)?.build()
            })
            .map(|bus| Handle { _bus: bus })
            .map_err(|err| format!("the test player cannot start on the session bus: {err}"))
    }
}

/// macOS: a small app (see player.swift) that swiftc compiles into the test folder. Each player is a separate app with
/// its own bundle ID, like a music app. It needs the Xcode Command Line Tools.
#[cfg(target_os = "macos")]
mod system {
    use super::{DELAY, Log, Path};
    use std::{
        fs,
        process::{Child, Command, Stdio},
    };

    pub const INTERFACE: &str = "Now Playing (MediaRemote through osascript, a Swift test app)";
    const SOURCE: &str = include_str!("player.swift");

    pub struct Handle(Child);

    impl Drop for Handle {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    pub fn start(name: &str, before: &str, log: Log, silence: &Path) -> Result<Handle, String> {
        let folder = silence.parent().ok_or("no test folder")?;
        let binary = folder.join("player");
        if !binary.exists() {
            let source = folder.join("player.swift");
            fs::write(&source, SOURCE).map_err(|err| err.to_string())?;
            let out = Command::new("swiftc").args(["-O", "-o"]).arg(&binary).arg(&source).output();
            match out {
                Ok(out) if out.status.success() => {}
                Ok(out) => return Err(format!("swiftc: {}", String::from_utf8_lossy(&out.stderr).trim())),
                Err(err) => return Err(format!("swiftc: {err}. Install the Xcode Command Line Tools: xcode-select --install")),
            }
        }
        // An executable in Contents/MacOS of a bundle gets the identity of the bundle, also when the test starts it.
        let contents = folder.join(format!("Moulin test {name}.app")).join("Contents");
        fs::create_dir_all(contents.join("MacOS")).map_err(|err| err.to_string())?;
        let plist = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>\
             <key>CFBundleIdentifier</key><string>app.moulin.e2e.{name}</string>\
             <key>CFBundleName</key><string>Moulin test {name}</string>\
             <key>CFBundleExecutable</key><string>player</string>\
             <key>CFBundlePackageType</key><string>APPL</string>\
             <key>LSUIElement</key><true/></dict></plist>\n"
        );
        fs::write(contents.join("Info.plist"), plist).map_err(|err| err.to_string())?;
        let executable = contents.join("MacOS").join("player");
        fs::copy(&binary, &executable).map_err(|err| err.to_string())?;
        let mut child = Command::new(&executable)
            .arg(silence)
            .arg(before)
            .arg(DELAY.as_millis().to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|err| format!("the test player: {err}"))?;
        super::follow(child.stdout.take().ok_or("no output of the test player")?, log);
        Ok(Handle(child))
    }
}

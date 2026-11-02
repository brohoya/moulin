//! Makes the media quiet during a dictation, and sets it back after the recording. Two modes:
//!
//! - Pause: pauses the media that plays (music, videos, podcasts), with the media interface of each system: the media
//!   sessions of Windows (smtc.rs), MPRIS on Linux (mpris.rs), and Now Playing on macOS (now_playing.rs).
//! - Lower: lowers the volume of the output device, so that the media keeps playing, more quietly (volume.rs).
//!
//! `app::set_state` makes the media quiet when the state becomes "recording", and sets it back when the state changes
//! from "recording". The calls wait for other apps, so a thread of Moulin runs them, in their order of arrival. Moulin
//! plays again only the media that it paused, so it never starts media that was not playing. It sets the volume back
//! only if the user did not change it during the recording.
#[cfg(target_os = "linux")]
mod mpris;
#[cfg(target_os = "macos")]
mod now_playing;
#[cfg(windows)]
mod smtc;
mod volume;

#[cfg(target_os = "linux")]
use mpris as system;
#[cfg(target_os = "macos")]
use now_playing as system;
#[cfg(windows)]
use smtc as system;

use crate::settings::MediaMode;
use std::{
    sync::{Mutex, PoisonError, mpsc},
    thread,
    time::Duration,
};

enum Job {
    /// Pauses the media, or lowers the volume to this percent.
    Quiet(MediaMode, u8),
    Restore,
    /// A restore, then a signal to the caller (see `restore_now`).
    RestoreAndSignal(mpsc::Sender<()>),
}

/// The queue of the media thread. The thread starts at the first pause or the first change of the volume.
static QUEUE: Mutex<Option<mpsc::Sender<Job>>> = Mutex::new(None);

/// Pauses the media that plays, or lowers the volume to `volume` percent, as `mode` says. The call returns at once.
pub fn quiet(mode: MediaMode, volume: u8) {
    if mode != MediaMode::Off {
        send(Job::Quiet(mode, volume), true);
    }
}

/// Plays again the media that Moulin paused, and sets the volume back. The call returns at once.
pub fn restore() {
    send(Job::Restore, false);
}

/// Restores the media, and waits for the end, for a maximum of `timeout`. Moulin calls this function when it quits,
/// so that a quit during a dictation does not keep the media paused or quiet.
pub fn restore_now(timeout: Duration) {
    let (done, wait) = mpsc::channel();
    if send(Job::RestoreAndSignal(done), false) {
        let _ = wait.recv_timeout(timeout);
    }
}

/// Gives a job to the media thread. Without a thread, Moulin changed no media, so only a "quiet" job starts the
/// thread. Returns false if the thread did not get the job.
fn send(job: Job, start: bool) -> bool {
    let mut queue = QUEUE.lock().unwrap_or_else(PoisonError::into_inner);
    if queue.is_none() && start {
        let (jobs, next) = mpsc::channel();
        match thread::Builder::new().name("moulin-media".into()).spawn(move || run(next)) {
            Ok(_) => *queue = Some(jobs),
            Err(err) => eprintln!("Moulin cannot change the media: {err}"),
        }
    }
    queue.as_ref().is_some_and(|jobs| jobs.send(job).is_ok())
}

/// The media thread. It keeps the players that it paused, and the volume that it lowered, until the next restore.
fn run(jobs: mpsc::Receiver<Job>) {
    let mut paused: Vec<system::Player> = Vec::new();
    let mut lowered: Option<volume::Lowered> = None;
    for job in jobs {
        let signal = match job {
            Job::Quiet(MediaMode::Pause, _) => {
                match system::pause() {
                    Ok(players) if players.is_empty() => {}
                    Ok(players) => {
                        println!("Media: paused {}", names(&players));
                        paused.extend(players);
                    }
                    Err(err) => eprintln!("Moulin cannot pause the media: {err}"),
                }
                continue;
            }
            // A second change before a restore keeps the volume of the first one, the volume of the user.
            Job::Quiet(MediaMode::Lower, percent) if lowered.is_none() => {
                match volume::lower(percent) {
                    Ok(Some(down)) => {
                        let (before, after) = down.percents();
                        println!("Media: volume lowered from {before}% to {after}%");
                        lowered = Some(down);
                    }
                    Ok(None) => {}
                    Err(err) => eprintln!("Moulin cannot lower the volume: {err}"),
                }
                continue;
            }
            Job::Quiet(..) => continue,
            Job::Restore => None,
            Job::RestoreAndSignal(done) => Some(done),
        };
        if !paused.is_empty() {
            println!("Media: playing again {}", names(&paused));
            system::resume(&std::mem::take(&mut paused));
        }
        if let Some(down) = lowered.take() {
            let (before, after) = down.percents();
            match volume::restore(&down) {
                Ok(true) => println!("Media: volume back to {before}%"),
                Ok(false) => println!("Media: the volume is not {after}% anymore, so the change of the user stays"),
                Err(err) => eprintln!("Moulin cannot set the volume back: {err}"),
            }
        }
        if let Some(done) = signal {
            let _ = done.send(());
        }
    }
}

fn names(players: &[system::Player]) -> String {
    players.iter().map(|player| player.name.as_str()).collect::<Vec<_>>().join(", ")
}

/// Reads the state of a player until it does not play, for a maximum of 1 second. An app applies a pause after a
/// moment, so a resume right after the pause (a very short dictation) waits for it. macOS waits in its script.
#[cfg(not(target_os = "macos"))]
fn settle<T>(read: impl Fn() -> T, plays: impl Fn(&T) -> bool) -> T {
    let started = std::time::Instant::now();
    loop {
        let state = read();
        if !plays(&state) || started.elapsed() >= Duration::from_secs(1) {
            return state;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// A program that did not give its output.
#[cfg(not(windows))]
#[derive(Debug)]
enum Failure {
    /// The program is not installed.
    Missing,
    Failed(String),
}

#[cfg(not(windows))]
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => f.write_str("the program is missing"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

/// Runs a program, for a maximum of 3 seconds, and returns its output. The C locale gives numbers with a point.
#[cfg(not(windows))]
fn output(program: &str, args: &[&str]) -> Result<String, Failure> {
    use std::{
        io::{ErrorKind, Read},
        process::{Command, Stdio},
        time::Instant,
    };
    let mut child = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| if err.kind() == ErrorKind::NotFound { Failure::Missing } else { Failure::Failed(format!("{program}: {err}")) })?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > Duration::from_secs(3) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Failure::Failed(format!("{program}: no answer in 3 seconds")));
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(err) => return Err(Failure::Failed(format!("{program}: {err}"))),
        }
    };
    let (mut out, mut error) = (String::new(), String::new());
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut out);
    }
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.read_to_string(&mut error);
    }
    if !status.success() {
        return Err(Failure::Failed(format!("{program}: {}", error.trim().lines().last().unwrap_or("failed"))));
    }
    Ok(out)
}

//! The command line options.
use crate::settings::MediaMode;
use std::path::PathBuf;

/// How Moulin runs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The app in the menu bar.
    App,
    /// "--e2e": one dictation with the sample WAV as the microphone, then a report (see e2e/).
    E2e,
    /// "--capture": a record of the request of the dictation button of chatgpt.com (see capture.rs).
    Capture,
}

pub struct Options {
    pub mode: Mode,
    /// "Moulin --toggle" starts or stops a dictation. A system shortcut can run it (for example on Wayland).
    pub toggle: bool,
    /// "--hold": the E2E test uses the hold mode.
    pub hold: bool,
    /// "--challenge": the E2E test gives the first upload a fake refusal of Cloudflare, so that Moulin runs the check.
    pub challenge: bool,
    /// "--media" or "--media=pause": the E2E test pauses the media during the dictation, and checks it with test players
    /// (see e2e/media.rs). "--media=lower": the test lowers the volume, and checks it (see e2e/volume.rs).
    pub media: Option<MediaMode>,
    /// "--sample=<file>": the WAV file that the E2E test plays instead of the microphone.
    pub sample: PathBuf,
    /// "--out=<folder>": the folder of the reports of the test modes.
    pub out_dir: PathBuf,
}

impl Options {
    pub fn parse() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let has = |flag: &str| args.iter().any(|a| a == flag);
        let value = |name: &str| args.iter().find_map(|a| a.strip_prefix(&format!("--{name}=")).map(PathBuf::from));
        let cwd = std::env::current_dir().unwrap_or_default();
        // "tauri dev" runs the app in src-tauri, so the defaults also look in the parent folder.
        let root = if cwd.ends_with("src-tauri") { cwd.join("..") } else { cwd };
        let mode = if has("--e2e") {
            Mode::E2e
        } else if has("--capture") {
            Mode::Capture
        } else {
            Mode::App
        };
        Self {
            mode,
            toggle: has("--toggle"),
            hold: has("--hold"),
            challenge: has("--challenge"),
            media: match args.iter().find_map(|a| a.strip_prefix("--media=")) {
                Some("lower") => Some(MediaMode::Lower),
                Some(_) => Some(MediaMode::Pause),
                None => has("--media").then_some(MediaMode::Pause),
            },
            sample: value("sample").unwrap_or_else(|| root.join("tests").join("fr-sample.wav")),
            out_dir: value("out").unwrap_or_else(|| root.join("artifacts")),
        }
    }
}

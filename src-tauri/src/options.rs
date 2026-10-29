//! The command line options.
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
            sample: value("sample").unwrap_or_else(|| root.join("fixtures").join("fr-sample.wav")),
            out_dir: value("out").unwrap_or_else(|| root.join("artifacts")),
        }
    }
}

//! The settings file: settings.json in the config folder of the app.
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// The shortcut of the dictation (for example "F5" or "Control+Alt+D"), or empty for no shortcut.
    pub hotkey: String,
    pub mode: HotkeyMode,
    /// Paste the text at the cursor. The text also stays in the clipboard.
    pub auto_paste: bool,
    pub open_at_login: bool,
    /// A device ID of cpal (for example "coreaudio:BuiltInMicrophoneDevice"), or empty for the system default.
    pub microphone_id: String,
    /// Show the pop-up during a dictation.
    pub show_overlay: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "F5".into(),
            mode: HotkeyMode::Toggle,
            auto_paste: true,
            open_at_login: true,
            microphone_id: String::new(),
            show_overlay: true,
        }
    }
}

/// How the shortcut controls a dictation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HotkeyMode {
    /// Hold the key while you speak, and release it to stop.
    Hold,
    /// Push the key to start, and push it again to stop. An unknown value in the file also gives this mode.
    #[default]
    #[serde(other)]
    Toggle,
}

/// A change from the settings window. A missing field keeps its value.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Patch {
    pub hotkey: Option<String>,
    pub mode: Option<HotkeyMode>,
    pub auto_paste: Option<bool>,
    pub open_at_login: Option<bool>,
    pub show_overlay: Option<bool>,
    pub microphone_id: Option<String>,
}

impl Settings {
    /// Applies a change, but not the shortcut: the shortcut must register first (see `hotkey::change`).
    pub fn apply(&mut self, patch: Patch) {
        if let Some(mode) = patch.mode {
            self.mode = mode;
        }
        if let Some(on) = patch.auto_paste {
            self.auto_paste = on;
        }
        if let Some(on) = patch.open_at_login {
            self.open_at_login = on;
        }
        if let Some(on) = patch.show_overlay {
            self.show_overlay = on;
        }
        if let Some(id) = patch.microphone_id {
            self.microphone_id = id;
        }
    }
}

pub fn load(file: &Path) -> Settings {
    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Settings::default(),
        Err(err) => {
            eprintln!("Cannot read {}: {err}. Moulin uses the default settings.", file.display());
            return Settings::default();
        }
    };
    serde_json::from_str(&text).unwrap_or_else(|err| {
        eprintln!("Cannot read {}: {err}. Moulin uses the default settings.", file.display());
        Settings::default()
    })
}

/// Writes a temporary file, then renames it, so that a crash never leaves a half-written file.
pub fn save(file: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder)?;
    }
    let tmp = file.with_extension("json.tmp");
    fs::write(&tmp, format!("{}\n", serde_json::to_string_pretty(settings)?))?;
    fs::rename(&tmp, file)
}

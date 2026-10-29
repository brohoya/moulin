// The settings file: settings.json in the config folder of the app.
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub hotkey: String,
    /// "toggle" or "hold".
    pub mode: String,
    pub auto_paste: bool,
    pub open_at_login: bool,
    /// A device ID of cpal (for example "coreaudio:BuiltInMicrophoneDevice"), or empty for the system default.
    pub microphone_id: String,
    pub show_overlay: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "F5".into(),
            mode: "toggle".into(),
            auto_paste: true,
            open_at_login: true,
            microphone_id: String::new(),
            show_overlay: true,
        }
    }
}

impl Settings {
    pub fn hold(&self) -> bool {
        self.mode == "hold"
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

// Write a temporary file, then rename it, so that a crash never leaves a half-written file.
pub fn save(file: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder)?;
    }
    let tmp = file.with_extension("json.tmp");
    fs::write(&tmp, format!("{}\n", serde_json::to_string_pretty(settings)?))?;
    fs::rename(&tmp, file)
}

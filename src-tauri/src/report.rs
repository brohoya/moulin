//! The report files of the test modes (E2E and capture).
use crate::app::now_ms;
use serde_json::Value;
use std::{fs, path::Path};

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

/// The time for a file name: the ISO time with "-" instead of ":" and ".".
pub fn stamp(iso: &str) -> String {
    iso.replace([':', '.'], "-")
}

/// Writes the report as indented JSON. Creates the folder if necessary.
pub fn write(file: &Path, report: &Value) -> Result<(), String> {
    if let Some(folder) = file.parent() {
        fs::create_dir_all(folder).map_err(|err| err.to_string())?;
    }
    let text = serde_json::to_string_pretty(report).map_err(|err| err.to_string())?;
    fs::write(file, format!("{text}\n")).map_err(|err| err.to_string())
}

pub fn file_name(path: &Path) -> Option<String> {
    path.file_name().map(|name| name.to_string_lossy().into_owned())
}

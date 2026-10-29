// Each command gets a permission "allow-<command>". The capabilities give each window only its own commands.
const COMMANDS: &[&str] = &[
    "overlay_clicked",
    "settings_get",
    "settings_set",
    "settings_capture",
    "account_sign_in",
    "account_sign_out",
    "open_accessibility",
    "microphones",
];

fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)))
        .expect("the Tauri build script failed");
}

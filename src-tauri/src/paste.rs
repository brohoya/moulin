//! Pastes the clipboard at the cursor of the active app: Cmd+V on macOS, Ctrl+V on Windows and Linux.
use std::{
    io::{ErrorKind, Read},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

enum Failure {
    Missing,
    Failed(String),
}

/// Runs a command with a time limit of 5 seconds. The error keeps the last line of the error output.
fn run(command: &str, args: &[&str]) -> Result<(), Failure> {
    let mut command_line = Command::new(command);
    command_line.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    // Windows: no console window. It could take the focus, and then Ctrl+V would go to it.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command_line.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command_line
        .spawn()
        .map_err(|err| if err.kind() == ErrorKind::NotFound { Failure::Missing } else { Failure::Failed(err.to_string()) })?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                let mut error = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = stderr.read_to_string(&mut error);
                }
                let last = error.trim().lines().last().unwrap_or_default().to_string();
                return Err(Failure::Failed(if last.is_empty() { status.to_string() } else { last }));
            }
            Ok(None) if started.elapsed() > Duration::from_secs(5) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Failure::Failed("no answer after 5 seconds".into()));
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(err) => return Err(Failure::Failed(err.to_string())),
        }
    }
}

pub fn paste() -> Result<(), String> {
    let fail = |failure| match failure {
        Failure::Missing => "the paste command is missing".to_string(),
        Failure::Failed(message) => message,
    };
    if cfg!(target_os = "macos") {
        // Needs the Accessibility permission. macOS also asks one time to let Moulin control System Events.
        return run("osascript", &["-e", "tell application \"System Events\" to keystroke \"v\" using command down"]).map_err(fail);
    }
    if cfg!(windows) {
        return run(
            "powershell.exe",
            &["-NoProfile", "-NonInteractive", "-Command", "(New-Object -ComObject WScript.Shell).SendKeys('^v')"],
        )
        .map_err(fail);
    }
    // Linux: xdotool on X11. On Wayland, wtype (wlroots desktops) or ydotool (needs its daemon).
    let wayland = crate::platform::wayland();
    let mut tools: Vec<(&str, Vec<&str>)> = Vec::new();
    if wayland {
        tools.push(("wtype", vec!["-M", "ctrl", "-k", "v", "-m", "ctrl"]));
        tools.push(("ydotool", vec!["key", "29:1", "47:1", "47:0", "29:0"]));
    }
    tools.push(("xdotool", vec!["key", "--clearmodifiers", "ctrl+v"]));
    // Try the next tool when a tool is missing, and also when the desktop refuses it (for example wtype on GNOME).
    let mut failures = Vec::new();
    for (command, args) in tools {
        match run(command, &args) {
            Ok(()) => return Ok(()),
            Err(Failure::Missing) => {}
            Err(Failure::Failed(message)) => failures.push(format!("{command}: {message}")),
        }
    }
    if !failures.is_empty() {
        return Err(failures.join("; "));
    }
    Err(if wayland { "install wtype or ydotool" } else { "install xdotool" }.into())
}

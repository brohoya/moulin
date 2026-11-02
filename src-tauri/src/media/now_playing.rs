//! macOS: Now Playing, the media of the app that macOS shows in the Control Center. Only that app gets the commands.
//! Since macOS 15.4, macOS gives the state of Now Playing only to its own programs, so osascript runs the check and
//! the command (see now_playing.js).
use serde::Deserialize;

const SCRIPT: &str = include_str!("now_playing.js");

/// The app that Moulin paused.
pub struct Player {
    pid: i64,
    app: String,
    /// The bundle ID of the app, for example "com.spotify.client".
    pub name: String,
}

/// What the script writes.
#[derive(Deserialize)]
struct Answer {
    result: String,
    #[serde(default)]
    pid: i64,
    #[serde(default)]
    app: String,
    /// The app of a web page, for example "com.apple.Safari" for the helper process of WebKit.
    #[serde(default)]
    host: String,
}

/// Pauses the Now Playing app if it plays. Returns the app if it paused.
pub fn pause() -> Result<Vec<Player>, String> {
    let answer = run(&["pause"])?;
    match answer.result.as_str() {
        "paused" => {
            let name = [&answer.host, &answer.app].into_iter().find(|id| !id.is_empty()).cloned();
            let name = name.unwrap_or_else(|| format!("process {}", answer.pid));
            Ok(vec![Player { pid: answer.pid, app: answer.app, name }])
        }
        "skipped" => Ok(Vec::new()),
        "unsupported" => Err("this version of macOS does not give the state of Now Playing (macOS 14 or later)".into()),
        other => Err(format!("the Now Playing app refused the command ({other})")),
    }
}

/// Plays the app again, if it is still the Now Playing app and if it is paused.
pub fn resume(players: &[Player]) {
    for player in players {
        match run(&["resume", &player.pid.to_string(), &player.app]) {
            Ok(answer) if answer.result == "resumed" => {}
            Ok(answer) => eprintln!("{} does not play again: {}", player.name, answer.result),
            Err(err) => eprintln!("{} does not play again: {err}", player.name),
        }
    }
}

/// Runs the script with osascript. The script waits 0.4 s for each command, in the limit of 3 seconds.
fn run(args: &[&str]) -> Result<Answer, String> {
    let out = super::output("/usr/bin/osascript", &[&["-l", "JavaScript", "-e", SCRIPT], args].concat()).map_err(|err| err.to_string())?;
    serde_json::from_str(out.trim()).map_err(|err| format!("osascript: {err}: {}", out.trim()))
}

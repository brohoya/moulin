//! Windows: the media sessions of the system, which the media controls of Windows show. Spotify, the browsers, and most
//! players have one. A browser has one session, for its active tab.
use std::{sync::mpsc, time::Duration};
use windows::{
    Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Sessions,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    },
    core::RuntimeType,
};
use windows_future::IAsyncOperation;

/// The maximum time to wait for an app. A frozen app does not stop the pauses and the resumes that follow.
const TIMEOUT: Duration = Duration::from_secs(2);

/// A session that Moulin paused.
pub struct Player {
    session: Session,
    /// The ID of the app, for example "Spotify.exe".
    pub name: String,
}

/// Pauses each session that plays. Returns the sessions that can pause: an app that does not answer in time can still
/// apply the pause later. The resume reads the state first, so it plays only a session that paused.
pub fn pause() -> Result<Vec<Player>, String> {
    let sessions = wait(Sessions::RequestAsync())?.GetSessions().map_err(|err| err.to_string())?;
    let mut paused = Vec::new();
    for session in sessions {
        let playing = session.GetPlaybackInfo().and_then(|info| info.PlaybackStatus()).is_ok_and(|status| status == Status::Playing);
        // Ok(false): the app refused the pause.
        if playing && !matches!(wait(session.TryPauseAsync()), Ok(false)) {
            let name = session.SourceAppUserModelId().map(|id| id.to_string()).unwrap_or_default();
            paused.push(Player { session, name });
        }
    }
    Ok(paused)
}

/// Plays the sessions again. A play goes only to a paused session: a play to a stopped session starts its media again
/// from the start, and a session that plays needs nothing.
pub fn resume(players: &[Player]) {
    for player in players {
        let status = super::settle(
            || player.session.GetPlaybackInfo().and_then(|info| info.PlaybackStatus()).ok(),
            |status| matches!(*status, Some(Status::Playing | Status::Changing)),
        );
        if status != Some(Status::Paused) {
            continue;
        }
        match wait(player.session.TryPlayAsync()) {
            Ok(true) => {}
            Ok(false) => eprintln!("{} does not play again: the app refused the command", player.name),
            Err(err) => eprintln!("{} does not play again: {err}", player.name),
        }
    }
}

/// Waits for the result of a call to another app, for a maximum of `TIMEOUT`.
fn wait<T: RuntimeType + Send + 'static>(call: windows::core::Result<IAsyncOperation<T>>) -> Result<T, String> {
    let operation = call.map_err(|err| err.to_string())?;
    let (result_tx, result) = mpsc::channel();
    operation
        .when(move |value| {
            let _ = result_tx.send(value);
        })
        .map_err(|err| err.to_string())?;
    match result.recv_timeout(TIMEOUT) {
        Ok(value) => value.map_err(|err| err.to_string()),
        Err(_) => {
            let _ = operation.Cancel();
            Err(format!("no answer in {} seconds", TIMEOUT.as_secs()))
        }
    }
}

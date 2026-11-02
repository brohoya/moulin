//! Linux: MPRIS, the D-Bus interface of the media players on all desktops. Spotify, Firefox, Chrome, VLC, and most
//! players have it.
use std::time::Duration;
use zbus::{
    blocking::{Connection, connection, fdo::DBusProxy},
    zvariant::OwnedValue,
};

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
/// playerctld sends its commands to another player, which has its own name. Moulin controls that player directly.
/// Moulin keeps plasma-browser-integration, which also shows the media of a browser: on KDE, it can be the only player
/// of the browser. Two pauses or two plays of the same media do no harm.
const RELAY: &str = "org.mpris.MediaPlayer2.playerctld";
/// The maximum time to wait for a player. A frozen player does not stop the pauses and the resumes that follow.
const TIMEOUT: Duration = Duration::from_secs(2);

/// A player that Moulin paused.
pub struct Player {
    /// The unique name of the connection of the player. It changes when the player starts again, so a player that
    /// starts again during the dictation gets no command.
    owner: String,
    /// The name of the player, for example "org.mpris.MediaPlayer2.spotify".
    pub name: String,
}

/// Pauses each player that plays. Returns the players that can pause: a player that does not answer in time can still
/// apply the pause later. The resume reads the state first, so it plays only a player that paused.
pub fn pause() -> Result<Vec<Player>, String> {
    let bus = connect()?;
    let dbus = DBusProxy::new(&bus).map_err(|err| err.to_string())?;
    let names = dbus.list_names().map_err(|err| err.to_string())?;
    let mut owners = Vec::new();
    let mut paused = Vec::new();
    for name in names.iter().filter(|name| name.starts_with(PREFIX) && name.as_str() != RELAY) {
        // A player can have more than one name. It gets one pause.
        let Ok(owner) = dbus.get_name_owner(name.inner().clone()) else { continue };
        let owner = owner.to_string();
        if owners.contains(&owner) {
            continue;
        }
        // An error answer: the player refused the pause.
        if status(&bus, &owner).is_ok_and(|status| status == "Playing")
            && !matches!(command(&bus, &owner, "Pause"), Err(zbus::Error::MethodError(..)))
        {
            paused.push(Player { owner: owner.clone(), name: name.to_string() });
        }
        owners.push(owner);
    }
    Ok(paused)
}

/// Plays the players again. A play goes only to a paused player: a play to a stopped player starts its track again
/// from the start, and a player that plays needs nothing.
pub fn resume(players: &[Player]) {
    let bus = match connect() {
        Ok(bus) => bus,
        Err(err) => return eprintln!("The media does not play again: {err}"),
    };
    for player in players {
        let status = super::settle(|| status(&bus, &player.owner).ok(), |status| status.as_deref() == Some("Playing"));
        if status.as_deref() != Some("Paused") {
            continue;
        }
        if let Err(err) = command(&bus, &player.owner, "Play") {
            eprintln!("{} does not play again: {err}", player.name);
        }
    }
}

fn connect() -> Result<Connection, String> {
    connection::Builder::session()
        .and_then(|builder| builder.method_timeout(TIMEOUT).build())
        .map_err(|err| format!("no D-Bus session bus ({err})"))
}

/// The state of a player: "Playing", "Paused", or "Stopped".
fn status(bus: &Connection, owner: &str) -> zbus::Result<String> {
    let reply = bus.call_method(Some(owner), PATH, Some("org.freedesktop.DBus.Properties"), "Get", &(PLAYER, "PlaybackStatus"))?;
    Ok(String::try_from(reply.body().deserialize::<OwnedValue>()?)?)
}

fn command(bus: &Connection, owner: &str, method: &str) -> zbus::Result<()> {
    bus.call_method(Some(owner), PATH, Some(PLAYER), method, &()).map(drop)
}

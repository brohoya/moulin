// Now Playing on macOS (see now_playing.rs). Since macOS 15.4, macOS gives the state of Now Playing only to its own
// programs. osascript is one of them, so osascript runs this script, and the script also sends the command.
// Only the Now Playing app (the app that macOS shows in the Control Center) gets the commands.
//
// "pause": pauses the Now Playing app if it plays.
// "resume <pid> <bundle ID>": plays the Now Playing app again, only if it is the same app and if it is paused, not
// stopped. A play to another app could start media that was not playing, or open the Music app.
//
// The script writes JSON: {"result": "paused" | "resumed" | "skipped" | "refused" | "unsupported", "pid", "app",
// "host"}. "host" is the app of a web page: for Safari, the Now Playing client is a helper process of WebKit.
function run(argv) {
  ObjC.import('Foundation');
  const answer = (result, client) =>
    JSON.stringify({ result, pid: client ? client.pid : 0, app: client ? client.app : '', host: client ? client.host : '' });
  const framework = $.NSBundle.bundleWithPath('/System/Library/PrivateFrameworks/MediaRemote.framework/');
  if (framework.isNil() || !framework.load) return answer('unsupported');
  const request = $.NSClassFromString('MRNowPlayingRequest');
  // macOS 13 has the class, but not these methods.
  if (request.isNil() || !request.respondsToSelector('localIsPlaying') || !request.respondsToSelector('localNowPlayingPlayerPath')) {
    return answer('unsupported');
  }
  ObjC.bindFunction('MRMediaRemoteSendCommand', ['bool', ['unsigned int', 'id']]);
  const PLAY = 0;
  const PAUSE = 1;

  // A property of an object, or undefined if this version of macOS does not have it.
  const read = (object, name) => (object.respondsToSelector(name) ? ObjC.unwrap(object[name]) : undefined);
  // The Now Playing app: its process ID and its bundle ID. A missing value is 0 or ''.
  const nowPlaying = () => {
    const path = request.localNowPlayingPlayerPath;
    const client = path.isNil() ? path : path.client;
    if (client.isNil()) return null;
    const pid = Number(read(client, 'processIdentifier')) || 0;
    const app = read(client, 'bundleIdentifier') || '';
    const host = read(client, 'parentApplicationBundleIdentifier') || '';
    return pid || app ? { pid, app, host } : null;
  };
  const playing = () => Boolean(ObjC.unwrap(request.localIsPlaying));
  // A stopped app (MRPlaybackState 3): its media ended, or the user stopped it. A play would start it again.
  const stopped = () => Number(read(request, 'localPlaybackState')) === 3;
  // The same app: the same process ID, or the same bundle ID when a process ID is missing.
  const same = (a, b) => Boolean(a && b) && (a.pid && b.pid ? a.pid === b.pid : a.app !== '' && a.app === b.app);
  // Sends a command. Some apps ignore the first command after a pause, so the command goes again if the same app did
  // not obey in 0.4 s. Returns false if MediaRemote refused the command.
  const send = (command, target, obeyed) => {
    if (!$.MRMediaRemoteSendCommand(command, $())) return false;
    delay(0.4);
    if (!obeyed() && same(nowPlaying(), target)) $.MRMediaRemoteSendCommand(command, $());
    return true;
  };

  const client = nowPlaying();
  if (argv[0] === 'pause') {
    if (!client || !playing()) return answer('skipped', client);
    return answer(send(PAUSE, client, () => !playing()) ? 'paused' : 'refused', client);
  }
  if (!same(client, { pid: Number(argv[1]) || 0, app: argv[2] || '' })) return answer('skipped', client);
  // An app applies a pause after a moment, so a resume right after the pause (a very short dictation) waits for it.
  for (let i = 0; i < 10 && playing(); i++) delay(0.1);
  if (playing() || stopped()) return answer('skipped', client);
  return answer(send(PLAY, client, playing) ? 'resumed' : 'refused', client);
}

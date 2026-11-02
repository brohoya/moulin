// A test player of the E2E test on macOS (see media.rs). It is a small app that plays a silent WAV file in a loop and
// answers the commands of Now Playing, like a music app: it applies each command after a delay. It writes the time
// (ms since 1970) and the state at each change of its state. Arguments: the WAV file, the state before the dictation
// ("playing" or "paused"), and the delay in ms.
import AVFoundation
import AppKit
import MediaPlayer

let arguments = CommandLine.arguments
guard arguments.count == 4, let delay = Double(arguments[3]),
    let audio = try? AVAudioPlayer(contentsOf: URL(fileURLWithPath: arguments[1]))
else {
    FileHandle.standardError.write("usage: player <WAV file> playing|paused <delay in ms>\n".data(using: .utf8)!)
    exit(1)
}
audio.numberOfLoops = -1

/// Plays or pauses, tells Now Playing, and writes the new state.
func set(playing: Bool) {
    if playing {
        audio.play()
    } else {
        audio.pause()
    }
    let center = MPNowPlayingInfoCenter.default()
    center.nowPlayingInfo = [MPMediaItemPropertyTitle: "Moulin test", MPNowPlayingInfoPropertyPlaybackRate: playing ? 1.0 : 0.0]
    center.playbackState = playing ? .playing : .paused
    print("\(Int64(Date().timeIntervalSince1970 * 1000)) \(playing ? "playing" : "paused")")
    fflush(stdout)
}

/// Applies a command after the delay, like a real app.
func later(playing: Bool) -> MPRemoteCommandHandlerStatus {
    DispatchQueue.main.asyncAfter(deadline: .now() + delay / 1000) { set(playing: playing) }
    return .success
}

let commands = MPRemoteCommandCenter.shared()
commands.playCommand.addTarget { _ in later(playing: true) }
commands.pauseCommand.addTarget { _ in later(playing: false) }
commands.togglePlayPauseCommand.addTarget { _ in later(playing: !audio.isPlaying) }

// No Dock icon. Now Playing gives the commands to an app, so the player runs the main loop of an app.
NSApplication.shared.setActivationPolicy(.accessory)
set(playing: true)
if arguments[2] == "paused" {
    set(playing: false)
}
// The test stops the player. The limit stops a player that the test forgot.
DispatchQueue.main.asyncAfter(deadline: .now() + 600) { exit(0) }
NSApplication.shared.run()

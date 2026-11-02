# A test player of the E2E test (see media.rs). It plays a silent WAV file in a loop with the WinRT MediaPlayer, so that
# Windows shows it as a media session, like the session of a music app. It writes the time (ms since 1970) and the
# state at each change of its state, from its state before the dictation, and its error if it stops. media.rs sets
# $Wav (a file URI) and $Start ("playing" or "paused") before this script.
$ErrorActionPreference = 'Stop'
function Write-State($state) {
  [Console]::Out.WriteLine(('{0} {1}' -f [DateTimeOffset]::Now.ToUnixTimeMilliseconds(), $state))
  [Console]::Out.Flush()
}
try {
  $null = [Windows.Media.Playback.MediaPlayer, Windows.Media, ContentType = WindowsRuntime]
  $null = [Windows.Media.Core.MediaSource, Windows.Media, ContentType = WindowsRuntime]
  $player = New-Object Windows.Media.Playback.MediaPlayer
  $player.Source = [Windows.Media.Core.MediaSource]::CreateFromUri([Uri]$Wav)
  $player.IsLoopingEnabled = $true
  function State { "$($player.PlaybackSession.PlaybackState)" }
  # The state before the dictation. Windows shows a session only after a start, so a paused player starts, then pauses.
  $player.Play()
  $ready = (Get-Date).AddSeconds(10)
  while ((State) -ne 'Playing' -and (Get-Date) -lt $ready) { Start-Sleep -Milliseconds 20 }
  if ($Start -eq 'paused') {
    $player.Pause()
    while ((State) -ne 'Paused' -and (Get-Date) -lt $ready) { Start-Sleep -Milliseconds 20 }
  }
  $last = ''
  # The test stops the process. The limit stops a player that the test forgot.
  $end = (Get-Date).AddMinutes(10)
  while ((Get-Date) -lt $end) {
    $state = State
    if ($state -ne $last) {
      Write-State $state.ToLower()
      $last = $state
    }
    Start-Sleep -Milliseconds 20
  }
  $player.Dispose()
} catch {
  Write-State "error: $($_.Exception.Message)"
  exit 1
}

#!/bin/sh
# Make fixtures/fr-sample.wav (48 kHz, mono, 16-bit) from the reference text fixtures/fr-sample.txt.
# Set VOICE to use a different French (France) voice of macOS.
set -eu
cd "$(dirname "$0")/.."
voice="${VOICE:-Thomas}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
say -v "$voice" -o "$tmp/sample.aiff" -f fixtures/fr-sample.txt
ffmpeg -y -loglevel error -i "$tmp/sample.aiff" -af volume=-3dB -ar 48000 -ac 1 -c:a pcm_s16le \
  -map_metadata -1 -fflags +bitexact -flags:a +bitexact fixtures/fr-sample.wav
ffprobe -v error -show_entries stream=sample_rate,channels,sample_fmt:format=duration -of compact fixtures/fr-sample.wav

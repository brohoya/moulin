#!/bin/sh
# Installs or updates Moulin from the latest GitHub release:
#   curl -fsSL https://brohoya.github.io/moulin/install.sh | sh
# macOS (Apple silicon): Moulin.app goes to ~/Applications.
# Linux (x86_64): the AppImage goes to ~/.local/bin/moulin.
# MOULIN_DIR changes the folder. MOULIN_NO_START=1 does not start Moulin after the install.
# MOULIN_URL changes the place of the downloads (for a test with a local build).
set -eu

REPO="brohoya/moulin"
URL="${MOULIN_URL:-https://github.com/$REPO/releases/latest/download}"

say() { printf '%s\n' "$*"; }
fail() { printf 'moulin: %s\n' "$*" >&2; exit 1; }

command -v curl >/dev/null 2>&1 || fail "this script needs curl."

# Quit a running Moulin before the new version replaces the old one.
quit_moulin() {
  pkill -TERM -x "$1" 2>/dev/null || return 0
  i=0
  while pgrep -x "$1" >/dev/null 2>&1 && [ "$i" -lt 50 ]; do sleep 0.2; i=$((i + 1)); done
  pkill -KILL -x "$1" 2>/dev/null || true
}

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)
    dir="${MOULIN_DIR:-$HOME/Applications}"
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    say "Downloading Moulin for macOS..."
    curl -fL --progress-bar "$URL/Moulin-mac-arm64.zip" -o "$tmp/Moulin.zip"
    ditto -x -k "$tmp/Moulin.zip" "$tmp/app"
    [ -d "$tmp/app/Moulin.app" ] || fail "the download does not contain Moulin.app."
    mkdir -p "$dir"
    if [ -d "$dir/Moulin.app" ]; then quit_moulin Moulin; fi
    rm -rf "$dir/Moulin.app"
    ditto "$tmp/app/Moulin.app" "$dir/Moulin.app"
    say "Installed $dir/Moulin.app"
    if [ -z "${MOULIN_NO_START:-}" ]; then open "$dir/Moulin.app"; fi
    ;;
  Darwin-*)
    fail "Moulin for macOS needs a Mac with Apple silicon."
    ;;
  Linux-x86_64)
    dir="${MOULIN_DIR:-$HOME/.local/bin}"
    mkdir -p "$dir"
    say "Downloading Moulin for Linux..."
    curl -fL --progress-bar "$URL/Moulin-linux-x86_64.AppImage" -o "$dir/moulin.download"
    chmod +x "$dir/moulin.download"
    if [ -f "$dir/moulin" ]; then quit_moulin Moulin; fi
    mv -f "$dir/moulin.download" "$dir/moulin"
    say "Installed $dir/moulin"
    if [ -z "${MOULIN_NO_START:-}" ]; then nohup "$dir/moulin" >/dev/null 2>&1 & fi
    ;;
  *)
    fail "this script installs Moulin on macOS (Apple silicon) and Linux (x86_64). For Windows, download the installer from https://github.com/$REPO/releases/latest"
    ;;
esac

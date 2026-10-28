#!/bin/sh
# Fast rebuild and reinstall on this Mac:
# 1. Build only the app folder (no dmg or zip).
# 2. Sign it with the local certificate of "npm run signing:setup", so that macOS keeps the permissions.
# 3. Quit Moulin, replace ~/Applications/Moulin.app, and start it with "open" (not as a child of the terminal).
set -eu
cd "$(dirname "$0")/.."
NAME="Moulin Local Signing"
KEYCHAIN="$HOME/Library/Keychains/moulin-signing.keychain-db"
APP="dist/mac-arm64/Moulin.app"
TARGET="$HOME/Applications/Moulin.app"

npx electron-builder --mac dir --publish never

if [ -f "$KEYCHAIN" ]; then
  security unlock-keychain -p "$(security find-generic-password -s moulin-signing -w)" "$KEYCHAIN"
  HASH=$(security find-identity -p codesigning "$KEYCHAIN" | awk -v name="\"$NAME\"" 'index($0, name) { print $2; exit }')
  # codesign finds an identity only in a keychain of the search list. Add the keychain for the signature, then restore the list.
  SEARCH_LIST=$(security list-keychains -d user | tr -d '"')
  trap 'security list-keychains -d user -s $SEARCH_LIST' EXIT
  security list-keychains -d user -s $SEARCH_LIST "$KEYCHAIN"
  codesign --force --deep --timestamp=none --sign "$HASH" --keychain "$KEYCHAIN" "$APP"
  security list-keychains -d user -s $SEARCH_LIST
  trap - EXIT
  codesign --verify --deep --strict "$APP"
else
  echo "No local signing key: the app keeps its ad-hoc signature, so macOS asks for the permissions again."
  echo "Run \"npm run signing:setup\" one time to stop that."
fi

# When the signature identity changes (for example from ad-hoc to the local certificate), the old permissions do not apply.
# Remove them, so that macOS asks again with clean entries instead of a switch that shows "on" but does nothing.
# An ad-hoc signature prints "# designated => cdhash ...", a certificate prints "designated => identifier ...".
requirement() { codesign -d -r- "$1" 2>&1 | sed -n 's/^#* *designated => //p'; }
OLD_REQUIREMENT=$(if [ -d "$TARGET" ]; then requirement "$TARGET"; fi)
NEW_REQUIREMENT=$(requirement "$APP")

# Quit the running Moulin. An old build can hang in its quit, so it gets 10 seconds, then a KILL.
pkill -TERM -x Moulin || true
i=0
while pgrep -x Moulin >/dev/null && [ $i -lt 50 ]; do sleep 0.2; i=$((i + 1)); done
pkill -KILL -x Moulin || true

rm -rf "$TARGET"
ditto "$APP" "$TARGET"
if [ -n "$OLD_REQUIREMENT" ] && [ "$OLD_REQUIREMENT" != "$NEW_REQUIREMENT" ]; then
  echo "The signature identity changed: macOS asks again for the microphone, Accessibility, and System Events."
  for service in Microphone Accessibility AppleEvents; do
    tccutil reset "$service" app.moulin >/dev/null 2>&1 || echo "Could not reset the $service permission. Remove the old Moulin entry in System Settings."
  done
else
  echo "The signature identity did not change: macOS keeps the permissions."
fi
open "$TARGET"
echo "Installed and started $TARGET"

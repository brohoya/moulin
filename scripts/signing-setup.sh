#!/bin/sh
# One time on this Mac: a self-signed code signing certificate in a separate keychain.
# macOS ties the permissions of Moulin (microphone, Accessibility, System Events) to the bundle ID and this certificate,
# so the builds that "npm run reinstall" signs with it keep these permissions.
# The login keychain keeps only the random password of the separate keychain (item "moulin-signing").
# Risk: a program that runs as you can use this key to sign an app that macOS treats as Moulin.
set -eu
NAME="Moulin Local Signing"
KEYCHAIN="$HOME/Library/Keychains/moulin-signing.keychain-db"

if [ -f "$KEYCHAIN" ]; then
  echo "The signing keychain exists already: $KEYCHAIN"
  security find-identity -p codesigning "$KEYCHAIN" | grep "$NAME" || true
  exit 0
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
PASSWORD=$(openssl rand -hex 24)
P12_PASSWORD=$(openssl rand -hex 16)

# A certificate for code signing only, valid for 10 years. The private key stays in the temporary folder until the import.
openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -subj "/CN=$NAME" \
  -addext "basicConstraints=critical,CA:false" -addext "keyUsage=critical,digitalSignature" -addext "extendedKeyUsage=critical,codeSigning" \
  -keyout "$TMP/key.pem" -out "$TMP/cert.pem" 2>/dev/null
openssl pkcs12 -export -inkey "$TMP/key.pem" -in "$TMP/cert.pem" -name "$NAME" -out "$TMP/identity.p12" -passout "pass:$P12_PASSWORD"

security create-keychain -p "$PASSWORD" "$KEYCHAIN"
security set-keychain-settings "$KEYCHAIN" # No automatic lock.
security unlock-keychain -p "$PASSWORD" "$KEYCHAIN"
# Only codesign can use the key, with no password prompt.
security import "$TMP/identity.p12" -k "$KEYCHAIN" -P "$P12_PASSWORD" -T /usr/bin/codesign >/dev/null
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$PASSWORD" "$KEYCHAIN" >/dev/null
security add-generic-password -U -a "$USER" -s moulin-signing -l "Moulin signing keychain" -w "$PASSWORD"

echo "Created the certificate \"$NAME\" in $KEYCHAIN"
security find-identity -p codesigning "$KEYCHAIN" | grep "$NAME"

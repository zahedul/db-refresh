#!/bin/bash
#
# Builds a shareable macOS release of DB Refresh: a signed .app inside a
# drag-to-install .dmg, plus a SHA-256 checksum, written to dist/.
#
# Usage:
#   scripts/build-release.sh
#
# Signing (optional environment variables):
#   SIGNING_IDENTITY  e.g. "Developer ID Application: Jane Doe (TEAMID1234)"
#                     Without it the app is signed ad-hoc: it runs on other Macs, but
#                     each user must allow it once (System Settings → Privacy & Security
#                     → Open Anyway).
#   NOTARY_PROFILE    Keychain profile created with `xcrun notarytool store-credentials`.
#                     With SIGNING_IDENTITY, the .dmg is notarized by Apple and opens
#                     without any warning.

set -Eeuo pipefail

cd "$(dirname "$0")/.."

APP_NAME="DB Refresh"
CRATE_NAME="db-refresh"
BUNDLE_DIR="target/dx/${CRATE_NAME}/bundle/macos/macos"
OUT_DIR="dist"

VERSION="$(grep -m1 '^version' Cargo.toml | cut -d '"' -f2)"
ARCH="$(uname -m)"
DMG_PATH="${OUT_DIR}/DB-Refresh-${VERSION}-${ARCH}.dmg"
SIGNING_IDENTITY="${SIGNING_IDENTITY:-}"
# Created by scripts/create-signing-certificate.sh; used when no Developer ID is set.
LOCAL_IDENTITY="DB Refresh Local Signing"
NOTARY_PROFILE="${NOTARY_PROFILE:-}"

# dx is installed by `cargo install dioxus-cli`; ~/.cargo/bin may not be on PATH.
DX="$(command -v dx || echo "$HOME/.cargo/bin/dx")"

step() {
    echo
    echo "==> $1"
}

fail() {
    echo "ERROR: $1" >&2
    exit 1
}

[[ -x "$DX" ]] || fail "Dioxus CLI not found. Install it with: cargo install dioxus-cli"
if [[ -n "$NOTARY_PROFILE" && -z "$SIGNING_IDENTITY" ]]; then
    fail "NOTARY_PROFILE needs SIGNING_IDENTITY: Apple only notarizes Developer ID signed apps."
fi

step "Building ${APP_NAME} ${VERSION} (${ARCH}) in release mode"
rm -rf "$BUNDLE_DIR"
"$DX" bundle --release --platform desktop --package-types macos

APP_PATH="${BUNDLE_DIR}/${APP_NAME}.app"
[[ -d "$APP_PATH" ]] || fail "Bundle not found at ${APP_PATH}"

# An "Apple Development" certificate (free with an Apple ID in Xcode) carries a Team ID,
# which gives the app a stable Keychain partition: "Always Allow" then survives updates.
APPLE_DEVELOPMENT_IDENTITY="$(security find-identity -v -p codesigning | grep -o '"Apple Development: [^"]*"' | head -1 | tr -d '"' || true)"

SIGNING_MODE="ad-hoc"
if [[ -n "$SIGNING_IDENTITY" ]]; then
    SIGNING_MODE="developer-id"
    step "Signing with ${SIGNING_IDENTITY} (hardened runtime)"
    codesign --force --options runtime --timestamp --sign "$SIGNING_IDENTITY" "$APP_PATH"
elif [[ -n "$APPLE_DEVELOPMENT_IDENTITY" ]]; then
    SIGNING_MODE="apple-development"
    step "Signing with ${APPLE_DEVELOPMENT_IDENTITY}"
    codesign --force --options runtime --sign "$APPLE_DEVELOPMENT_IDENTITY" "$APP_PATH"
elif security find-identity -v -p codesigning | grep -q "\"$LOCAL_IDENTITY\""; then
    # A stable identity keeps Keychain "Always Allow" working across builds.
    SIGNING_MODE="local"
    step "Signing with ${LOCAL_IDENTITY}"
    codesign --force --options runtime --sign "$LOCAL_IDENTITY" "$APP_PATH"
else
    # The compiler only signs the executable; sign the whole bundle so macOS doesn't
    # report the downloaded app as "damaged".
    step "Signing ad-hoc (no Developer ID or local certificate; see create-signing-certificate.sh)"
    codesign --force --sign - "$APP_PATH"
fi
codesign --verify --strict --verbose=1 "$APP_PATH"

step "Creating ${DMG_PATH}"
mkdir -p "$OUT_DIR"
STAGING_DIR="$(mktemp -d)"
trap 'rm -rf "$STAGING_DIR"' EXIT
cp -R "$APP_PATH" "$STAGING_DIR/"
# The user guide sits next to the app, so people see it when they open the .dmg.
cp docs/USER_GUIDE.md "$STAGING_DIR/User Guide.md"
# Shown next to the app in the opened .dmg, so users can drag it to install.
ln -s /Applications "$STAGING_DIR/Applications"
rm -f "$DMG_PATH"
hdiutil create \
    -volname "$APP_NAME" \
    -srcfolder "$STAGING_DIR" \
    -fs HFS+ \
    -format UDZO \
    -quiet \
    "$DMG_PATH"

if [[ -n "$SIGNING_IDENTITY" ]]; then
    codesign --force --timestamp --sign "$SIGNING_IDENTITY" "$DMG_PATH"
fi

if [[ -n "$NOTARY_PROFILE" ]]; then
    step "Notarizing with Apple (usually a few minutes)"
    xcrun notarytool submit "$DMG_PATH" --keychain-profile "$NOTARY_PROFILE" --wait
    xcrun stapler staple "$DMG_PATH"
fi

step "Writing checksum"
(cd "$OUT_DIR" && shasum -a 256 "$(basename "$DMG_PATH")" > "$(basename "$DMG_PATH").sha256")

echo
echo "=========================================="
echo " Release ready"
echo "=========================================="
echo " DMG      : ${DMG_PATH} ($(du -h "$DMG_PATH" | cut -f1 | xargs))"
echo " SHA-256  : $(cut -d ' ' -f1 "${DMG_PATH}.sha256")"
if [[ -n "$NOTARY_PROFILE" ]]; then
    echo " Signing  : Developer ID, notarized (opens without warnings)"
elif [[ -n "$SIGNING_IDENTITY" ]]; then
    echo " Signing  : Developer ID, not notarized (users see one warning)"
elif [[ "$SIGNING_MODE" == "apple-development" ]]; then
    echo " Signing  : Apple Development (stable Keychain access on this Mac; other Macs: Open Anyway once)"
elif [[ "$SIGNING_MODE" == "local" ]]; then
    echo " Signing  : local certificate (no Team ID: macOS asks once per update for the saved passwords)"
else
    echo " Signing  : ad-hoc (users allow it once: System Settings → Privacy & Security → Open Anyway)"
fi
echo " Runs on  : macOS 13+ on ${ARCH} Macs"
echo "=========================================="

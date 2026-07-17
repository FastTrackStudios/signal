#!/usr/bin/env bash
# Build a release IPA and upload it to TestFlight via the App Store Connect
# API. Runs in the Mac's GUI login session (codesign needs the unlocked
# login keychain). One-time prerequisites (create in the browser / Xcode):
#   1. App record in App Store Connect for the bundle id (app.fasttrackstudio)
#   2. An "Apple Distribution" certificate in the keychain
#      (Xcode → Settings → Accounts → Manage Certificates → + )
# The App Store Connect API key lives in ~/.appstoreconnect (config.env).
#
#   ./ios/deploy-testflight.sh
#
# Each upload needs a higher build number than the last; we stamp it from
# the current unix time (passed in, since dx/nix can't read the clock).
set -euo pipefail

cd "$(dirname "$0")/.."

TEAM_ID="${TEAM_ID:-28C2G63DA7}"
NIX="${NIX:-/run/current-system/sw/bin/nix}"
BIN_IOS="$HOME/bin-ios"
mkdir -p "$BIN_IOS"
ln -sf /usr/bin/xcrun "$BIN_IOS/xcrun"

# shellcheck disable=SC1090
source "$HOME/.appstoreconnect/config.env"

# Distribution signing identity (Apple Distribution).
SIGN_ID="$(security find-identity -v -p codesigning \
    | awk -F'"' '/Apple Distribution/{print $2; exit}')"
[ -n "$SIGN_ID" ] || { echo "ERROR: no 'Apple Distribution' cert in the keychain. Create one in Xcode." >&2; exit 1; }
echo "=== distribution identity: $SIGN_ID ==="

echo "=== App Store provisioning profile ==="
PROFILE="$(PROFILE_TYPE=IOS_APP_STORE CERT_TYPE=DISTRIBUTION \
    ruby "$(dirname "$0")/mint-dev-profile.rb" - | awk -F= '/PROFILE_PATH=/{print $2}')"
echo "profile: $PROFILE"

echo "=== building release ==="
"$NIX" develop "$(git rev-parse --show-toplevel)" -c bash -c \
    "unset DEVELOPER_DIR SDKROOT; export PATH=$BIN_IOS:\$PATH; \
     dx build --platform ios --device --release --no-default-features --features signal-guitar" \
    2>&1 | tail -2

APP="$(git rev-parse --show-toplevel)/target/dx/fasttrackstudio/release/ios/Fasttrackstudio.app"
BUNDLE="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Info.plist")"

# Info.plist: usage strings + file sharing + a fresh build number.
BUILD_NO="${BUILD_NO:-$(date +%s)}"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $BUILD_NO" "$APP/Info.plist"
/usr/libexec/PlistBuddy -c "Add :NSMicrophoneUsageDescription string 'Processes your guitar signal from the connected audio interface or microphone.'" "$APP/Info.plist" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Add :UIFileSharingEnabled bool true" "$APP/Info.plist" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Add :LSSupportsOpeningDocumentsInPlace bool true" "$APP/Info.plist" 2>/dev/null || true
# TestFlight requires ITSAppUsesNonExemptEncryption declared (false = no
# non-standard crypto → no export-compliance docs).
/usr/libexec/PlistBuddy -c "Add :ITSAppUsesNonExemptEncryption bool false" "$APP/Info.plist" 2>/dev/null || true
echo "=== app: $APP ($BUNDLE) build $BUILD_NO ==="

echo "=== signing (distribution) ==="
cp "$PROFILE" "$APP/embedded.mobileprovision"
security cms -D -i "$PROFILE" > /tmp/fts-prof.plist
/usr/libexec/PlistBuddy -x -c "Print :Entitlements" /tmp/fts-prof.plist > /tmp/fts-ent.plist
find "$APP" \( -name "*.dylib" -o -name "*.framework" \) -print0 \
    | while IFS= read -r -d '' f; do codesign --force --sign "$SIGN_ID" --timestamp "$f"; done
codesign --force --sign "$SIGN_ID" --entitlements /tmp/fts-ent.plist --timestamp "$APP"
codesign --verify --deep --strict "$APP"

echo "=== packaging IPA ==="
WORK="$(mktemp -d)"
mkdir -p "$WORK/Payload"
cp -R "$APP" "$WORK/Payload/"
IPA="$WORK/FastTrackStudio.ipa"
( cd "$WORK" && zip -qry "$IPA" Payload )

echo "=== uploading to TestFlight ==="
xcrun altool --upload-app -t ios -f "$IPA" \
    --apiKey "$ASC_KEY_ID" --apiIssuer "$ASC_ISSUER_ID"
echo "=== DONE — build $BUILD_NO uploaded; it appears in TestFlight after Apple processes it (~5-15 min) ==="

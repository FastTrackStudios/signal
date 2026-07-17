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

# Distribution signing identity — create it via the API + import into the
# login keychain if it isn't there yet (no Xcode UI needed).
if ! security find-identity -v -p codesigning | grep -q "Apple Distribution"; then
    echo "=== creating Apple Distribution certificate ==="
    eval "$(ruby "$(dirname "$0")/mint-dist-cert.rb" | grep -E '^DIST_(KEY|CER)=')"
    # Bundle key + cert into a .p12 and import (-A: allow all apps, so
    # codesign uses it without a per-run keychain prompt).
    openssl x509 -inform DER -in "$DIST_CER" -out /tmp/fts-dist.pem
    openssl pkcs12 -export -legacy -inkey "$DIST_KEY" -in /tmp/fts-dist.pem \
        -name "Apple Distribution" -out /tmp/fts-dist.p12 -passout pass:fts
    security import /tmp/fts-dist.p12 -k "$HOME/Library/Keychains/login.keychain-db" \
        -P fts -A -T /usr/bin/codesign
    rm -f /tmp/fts-dist.pem /tmp/fts-dist.p12
fi
SIGN_ID="$(security find-identity -v -p codesigning \
    | awk -F'"' '/Apple Distribution/{print $2; exit}')"
[ -n "$SIGN_ID" ] || { echo "ERROR: distribution identity still missing after import." >&2; exit 1; }
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

# Info.plist: usage strings + file sharing + versions. The App Store
# requires CFBundleShortVersionString to be 1-3 period-separated integers
# (the crate's "0.0.1-alpha" is rejected); CFBundleVersion just has to climb.
BUILD_NO="${BUILD_NO:-$(date +%s)}"
MARKETING_VER="${MARKETING_VER:-0.0.1}"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $MARKETING_VER" "$APP/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $BUILD_NO" "$APP/Info.plist"
# Minimum OS — App Store rejects a bundle without it (dx doesn't emit one).
/usr/libexec/PlistBuddy -c "Set :MinimumOSVersion 15.0" "$APP/Info.plist" 2>/dev/null \
    || /usr/libexec/PlistBuddy -c "Add :MinimumOSVersion string 15.0" "$APP/Info.plist"
/usr/libexec/PlistBuddy -c "Add :NSMicrophoneUsageDescription string 'Processes your guitar signal from the connected audio interface or microphone.'" "$APP/Info.plist" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Add :UIFileSharingEnabled bool true" "$APP/Info.plist" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Add :LSSupportsOpeningDocumentsInPlace bool true" "$APP/Info.plist" 2>/dev/null || true
# TestFlight requires ITSAppUsesNonExemptEncryption declared (false = no
# non-standard crypto → no export-compliance docs).
/usr/libexec/PlistBuddy -c "Add :ITSAppUsesNonExemptEncryption bool false" "$APP/Info.plist" 2>/dev/null || true

# Home-screen icon (dx emits none).
ICONS_DIR="$(git rev-parse --show-toplevel)/apps/fasttrackstudio/ios/Assets.xcassets"
if [ -d "$ICONS_DIR" ]; then
    actool "$ICONS_DIR" --compile "$APP" --platform iphoneos \
        --minimum-deployment-target 15.0 --app-icon AppIcon \
        --output-partial-info-plist /tmp/fts-icon.plist >/dev/null 2>&1 \
        && /usr/libexec/PlistBuddy -c "Merge /tmp/fts-icon.plist" "$APP/Info.plist" 2>/dev/null \
        || echo "warn: app-icon compile skipped"
fi
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
# ditto (not zip) — preserves the _CodeSignature/CodeResources symlink that
# altool requires; a plain zip breaks it.
( cd "$WORK" && ditto -c -k --sequesterRsrc --keepParent Payload "$IPA" )

echo "=== uploading to TestFlight ==="
xcrun altool --upload-app -t ios -f "$IPA" \
    --apiKey "$ASC_KEY_ID" --apiIssuer "$ASC_ISSUER_ID"
echo "=== DONE — build $BUILD_NO uploaded; it appears in TestFlight after Apple processes it (~5-15 min) ==="

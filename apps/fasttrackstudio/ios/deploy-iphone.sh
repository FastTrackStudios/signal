#!/usr/bin/env bash
# Build (device arch) + sign + install + launch the iPhone app on a paired
# device over the cable. Must run in the Mac's GUI login session — codesign
# needs the unlocked login keychain (SSH sessions can't reach it).
#
#   ./ios/deploy-iphone.sh
#
# Config via env (defaults are voyager's):
#   PHONE_UDID   devicectl device id (xcrun devicectl list devices)
#   SIGN_ID      codesign identity SHA-1 (security find-identity -p codesigning)
#   PROFILE      path to a .mobileprovision covering the bundle id
#   TEAM_ID      Apple Developer team id
set -euo pipefail

cd "$(dirname "$0")/.."

PHONE_UDID="${PHONE_UDID:-06AFA75D-D8FC-57AB-B91A-E06CCC7B2DD9}"  # devicectl id
SIGN_ID="${SIGN_ID:-991CD4E0B4B81F4DB8AAF0B8D7DC895560A69715}"
TEAM_ID="${TEAM_ID:-28C2G63DA7}"
NIX="${NIX:-/run/current-system/sw/bin/nix}"

BIN_IOS="$HOME/bin-ios"
mkdir -p "$BIN_IOS"
ln -sf /usr/bin/xcrun "$BIN_IOS/xcrun"

# Fail fast if the phone isn't connected (else the device queries hang).
# (No `timeout` here — it isn't present on stock macOS.)
echo "=== checking device ==="
if ! xcrun devicectl list devices 2>/dev/null | grep -q "$PHONE_UDID"; then
    echo "ERROR: device $PHONE_UDID not reachable. Plug in the iPhone," \
         "unlock it, and make sure it's trusted. Then rerun." >&2
    exit 1
fi

# Ensure a development profile that covers THIS device + the bundle id,
# minted via the App Store Connect API (config in ~/.appstoreconnect).
echo "=== provisioning profile ==="
HW_UDID="$(xcrun devicectl device info details --device "$PHONE_UDID" 2>/dev/null | awk '/udid:/{print $NF; exit}')"
[ -n "$HW_UDID" ] || { echo "ERROR: couldn't read device UDID (is it unlocked?)" >&2; exit 1; }
# shellcheck disable=SC1090
source "$HOME/.appstoreconnect/config.env"
PROFILE="$(ruby "$(dirname "$0")/mint-dev-profile.rb" "$HW_UDID" | awk -F= '/PROFILE_PATH=/{print $2}')"
echo "profile: $PROFILE"

echo "=== building (device arch) ==="
"$NIX" develop "$(git rev-parse --show-toplevel)" -c bash -c \
    "unset DEVELOPER_DIR SDKROOT; export PATH=$BIN_IOS:\$PATH; \
     dx build --platform ios --device --no-default-features --features signal-guitar,signal-keys-rig" \
    2>&1 | tail -2

APP="$(git rev-parse --show-toplevel)/target/dx/fasttrackstudio/debug/ios/Fasttrackstudio.app"
BUNDLE="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Info.plist")"
echo "=== app: $APP ($BUNDLE) ==="

# Runtime per-screen rotation (ios_orientation.rs) needs both orientations
# in the plist, which dx already emits. Add the mic usage string + Files-app
# sharing so Documents/FastTrackStudio (config + user sample packs) is
# visible and manageable on-device.
/usr/libexec/PlistBuddy -c "Add :NSMicrophoneUsageDescription string 'Processes your guitar signal from the connected audio interface or microphone.'" "$APP/Info.plist" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Add :UIFileSharingEnabled bool true" "$APP/Info.plist" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Add :LSSupportsOpeningDocumentsInPlace bool true" "$APP/Info.plist" 2>/dev/null || true

# Home-screen icon: compile the FTS asset catalog into the bundle (dx emits
# no app icon). actool produces Assets.car + a partial plist (CFBundleIcons /
# asset-name keys) we merge in.
ICONS_DIR="$(git rev-parse --show-toplevel)/apps/fasttrackstudio/ios/Assets.xcassets"
if [ -d "$ICONS_DIR" ]; then
    actool "$ICONS_DIR" --compile "$APP" --platform iphoneos \
        --minimum-deployment-target 15.0 --app-icon AppIcon \
        --output-partial-info-plist /tmp/fts-icon.plist >/dev/null 2>&1 \
        && /usr/libexec/PlistBuddy -c "Merge /tmp/fts-icon.plist" "$APP/Info.plist" 2>/dev/null \
        || echo "warn: app-icon compile skipped"
fi

echo "=== signing ==="
cp "$PROFILE" "$APP/embedded.mobileprovision"
# Entitlements come straight from the profile, so signature and embedded
# profile can never disagree.
security cms -D -i "$PROFILE" > /tmp/fts-prof.plist
/usr/libexec/PlistBuddy -x -c "Print :Entitlements" /tmp/fts-prof.plist > /tmp/fts-ent.plist
# Sign nested code first (dylibs/frameworks), then the app bundle.
find "$APP" \( -name "*.dylib" -o -name "*.framework" \) -print0 \
    | while IFS= read -r -d '' f; do
        codesign --force --sign "$SIGN_ID" --timestamp=none "$f"
    done
codesign --force --sign "$SIGN_ID" --entitlements /tmp/fts-ent.plist --timestamp=none "$APP"
codesign --verify --deep --verbose=2 "$APP"

echo "=== installing on device ==="
xcrun devicectl device install app --device "$PHONE_UDID" "$APP"
echo "=== launching ==="
xcrun devicectl device process launch --terminate-existing --device "$PHONE_UDID" "$BUNDLE"
echo "=== DONE — check the phone ==="

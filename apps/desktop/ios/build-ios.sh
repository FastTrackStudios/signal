#!/usr/bin/env bash
# Build the iPhone app (Session — setlists, songs, charts, the guide).
#
# Run on a Mac inside the repo's nix dev shell. The env dance is REQUIRED:
# nixpkgs ships a fake xcbuild `xcrun` and its SDK env breaks Xcode's, so
# iOS cross-compiles need the real xcrun first on PATH and the nix SDK vars
# unset (the flake's CARGO_TARGET_*_LINKER / CC_* handle the rest).
#
#   cd apps/desktop && ./ios/build-ios.sh [--sim <udid>]
#
# With --sim, also installs + relaunches on that simulator.
set -euo pipefail

cd "$(dirname "$0")/.."

BIN_IOS="$HOME/bin-ios"
mkdir -p "$BIN_IOS"
ln -sf /usr/bin/xcrun "$BIN_IOS/xcrun"
ln -sf /usr/bin/xcodebuild "$BIN_IOS/xcodebuild"

unset DEVELOPER_DIR SDKROOT
export PATH="$BIN_IOS:$PATH"

dx build --platform ios --no-default-features --features session-domain,charts

# NOTE: verify this against dx's actual output dir/app name on first run —
# dx derives it from the crate name (`session-desktop`) since the rename.
APP="$(cd ../.. && pwd)/target/dx/session-desktop/debug/ios/Session-desktop.app"

# cpal's duplex audio session can request mic access even when only
# output is used; Apple requires the usage string to exist regardless.
/usr/libexec/PlistBuddy -c "Add :NSMicrophoneUsageDescription string 'Used for the audio session during setlist playback.'" "$APP/Info.plist" 2>/dev/null || true
# Files-app visibility for the setlist library (Documents/Session).
/usr/libexec/PlistBuddy -c "Add :UIFileSharingEnabled bool true" "$APP/Info.plist" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Add :LSSupportsOpeningDocumentsInPlace bool true" "$APP/Info.plist" 2>/dev/null || true

echo "built: $APP"

if [[ "${1:-}" == "--sim" && -n "${2:-}" ]]; then
    xcrun simctl install "$2" "$APP"
    xcrun simctl launch --terminate-running-process "$2" \
        "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Info.plist")"
    echo "launched on simulator $2"
fi

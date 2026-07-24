#!/usr/bin/env bash
# Build the iPhone app (the in-process guitar rig) and lock it to landscape.
#
# Run on a Mac inside the repo's nix dev shell. The env dance is REQUIRED:
# nixpkgs ships a fake xcbuild `xcrun` and its SDK env breaks Xcode's, so
# iOS cross-compiles need the real xcrun first on PATH and the nix SDK vars
# unset (the flake's CARGO_TARGET_*_LINKER / CC_* handle the rest).
#
#   cd apps/fasttrackstudio && ./ios/build-ios.sh [--sim <udid>]
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

dx build --platform ios --no-default-features --features signal-guitar,signal-keys-rig

APP="$(cd ../.. && pwd)/target/dx/fasttrackstudio/debug/ios/Fasttrackstudio.app"

# The app rotates per-screen at runtime (ios_orientation.rs), so Info.plist
# keeps BOTH portrait and landscape (dx's default) — no orientation patch.
# Add the mic/audio-interface usage string the rig needs.
/usr/libexec/PlistBuddy -c "Add :NSMicrophoneUsageDescription string 'Processes your guitar signal from the connected audio interface or microphone.'" "$APP/Info.plist" 2>/dev/null || true

echo "built: $APP"

if [[ "${1:-}" == "--sim" && -n "${2:-}" ]]; then
    xcrun simctl install "$2" "$APP"
    xcrun simctl launch --terminate-running-process "$2" \
        "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Info.plist")"
    echo "launched on simulator $2"
fi

#!/usr/bin/env bash
# Build the iPhone app (the in-process guitar rig) and lock it to landscape.
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

HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE/.."

BIN_IOS="$HOME/bin-ios"
mkdir -p "$BIN_IOS"
ln -sf /usr/bin/xcrun "$BIN_IOS/xcrun"
ln -sf /usr/bin/xcodebuild "$BIN_IOS/xcodebuild"

unset DEVELOPER_DIR SDKROOT
export PATH="$BIN_IOS:$PATH"
# The floor the devshell sets (nix/modules/toolchain.nix) and Info.plist's
# MinimumOSVersion: a build outside nix otherwise links for iOS 10 while
# the C++ objects (NAM's Eigen) were compiled for the SDK's own version, and
# `___chkstk_darwin` is missing at the link.
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-15.0}"

dx build --platform ios --no-default-features --features signal-guitar,signal-keys-rig,tone3000

# dx names the bundle from the package (SignalDesktop.app today; it has
# changed between dx versions), so take the newest one it wrote.
APP="$(ls -dt "$(cd ../.. && pwd)"/target/dx/signal-desktop/debug/ios/*.app 2>/dev/null | head -1)"
[ -n "$APP" ] && [ -f "$APP/Info.plist" ] || { echo "ERROR: dx produced no app" >&2; exit 1; }

# Info.plist (scene manifest, dark, usage strings) and frame's faces —
# the same as the TestFlight build writes.
# shellcheck source=app-plist.sh
source "$HERE/app-plist.sh"
signal_app_plist "$APP"
signal_app_faces "$APP"

# IPAD_LANDSCAPE=1 (development only — TestFlight keeps every orientation
# and multitasking): the iPad held sideways, full screen. A simulator here
# has no Simulator app to turn it, and iPadOS's windowing mode refuses a
# programmatic turn.
if [[ "${IPAD_LANDSCAPE:-0}" == "1" ]]; then
    PB=/usr/libexec/PlistBuddy
    $PB -c "Delete :UISupportedInterfaceOrientations~ipad" "$APP/Info.plist" 2>/dev/null || true
    $PB -c "Add :UISupportedInterfaceOrientations~ipad array" "$APP/Info.plist"
    $PB -c "Add :UISupportedInterfaceOrientations~ipad:0 string UIInterfaceOrientationLandscapeRight" "$APP/Info.plist"
    $PB -c "Add :UISupportedInterfaceOrientations~ipad:1 string UIInterfaceOrientationLandscapeLeft" "$APP/Info.plist"
    $PB -c "Delete :UIRequiresFullScreen" "$APP/Info.plist" 2>/dev/null || true
    $PB -c "Add :UIRequiresFullScreen bool true" "$APP/Info.plist"
    echo "iPad: landscape only, full screen (development)"
fi

echo "built: $APP"

if [[ "${1:-}" == "--sim" && -n "${2:-}" ]]; then
    xcrun simctl install "$2" "$APP"
    xcrun simctl launch --terminate-running-process "$2" \
        "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Info.plist")"
    echo "launched on simulator $2"
fi

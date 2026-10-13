#!/usr/bin/env bash
# Wrap the release-fast build in a macOS app bundle: `Signal.app`, the guitar
# rig, launched from Finder / the Dock / Spotlight like any app.
#
#   scripts/mac-app.sh                 build target/Signal.app
#   scripts/mac-app.sh --install       …and copy it to ~/Applications
#   SIGNAL_WINDOW_SIZE=1512x945 scripts/mac-app.sh --install
#                                      open at that size, not maximized
#                                      (1512x945: a MacBook Pro 14"'s screen
#                                      less its menu bar — lay out on a big
#                                      screen what the laptop will show)
#
# The binary is copied INTO the bundle (not linked): macOS ties a process to
# its bundle by the executable's path, so a binary outside it would run
# nameless — no Dock name, no microphone prompt of its own, no window owner
# for screen tools. The launcher only adds the rig's arguments and log file.
set -euo pipefail
cd "$(dirname "$0")/.."

bin=target/release-fast/signal-desktop
[ -x "$bin" ] || { echo "no $bin — run \`just app\` first" >&2; exit 1; }

app=target/Signal.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/signal-desktop"

cat > "$app/Contents/MacOS/Signal" <<'SH'
#!/bin/bash
# The guitar rig, its log beside the other apps' logs.
logs="$HOME/Library/Logs/Signal"
mkdir -p "$logs"
export RUST_LOG="${RUST_LOG:-info,vox_core=warn,schema_deser=off}"
# The faces ride inside the bundle: the build tree may be on an external
# volume, and macOS asks before every app that reads one.
export SIGNAL_FRAME_DIR="${SIGNAL_FRAME_DIR:-$(cd "$(dirname "$0")/../Resources/faces" && pwd)}"
__WINDOW__
exec "$(dirname "$0")/signal-desktop" --guitar "$@" >>"$logs/signal.log" 2>&1
SH
# The window size, baked in at bundle time (Finder passes no environment).
if [ -n "${SIGNAL_WINDOW_SIZE:-}" ]; then
    window="export FTS_WINDOW_SIZE=\"\${FTS_WINDOW_SIZE:-$SIGNAL_WINDOW_SIZE}\" FTS_WINDOW_MAXIMIZED=\"\${FTS_WINDOW_MAXIMIZED:-0}\""
else
    window=""
fi
sed -i '' "s|^__WINDOW__\$|$window|" "$app/Contents/MacOS/Signal"
chmod +x "$app/Contents/MacOS/Signal"

# The rig's faces (the frame checkout beside this repo — what
# `frame_surface::design_dir` finds in a dev build).
faces=../frame/examples/plugins/rig-faces
if [ -d "$faces" ]; then
    mkdir -p "$app/Contents/Resources/faces"
    cp -R "$faces" "$app/Contents/Resources/faces/"
else
    echo "no $faces — the app will draw plain cards" >&2
fi

icon=apps/desktop/ios/Assets.xcassets/AppIcon.appiconset/icon-1024.png
if [ -f "$icon" ]; then
    set_dir=$(mktemp -d)/Signal.iconset
    mkdir -p "$set_dir"
    for s in 16 32 128 256 512; do
        sips -z $s $s "$icon" --out "$set_dir/icon_${s}x${s}.png" >/dev/null
        sips -z $((s * 2)) $((s * 2)) "$icon" --out "$set_dir/icon_${s}x${s}@2x.png" >/dev/null
    done
    iconutil -c icns "$set_dir" -o "$app/Contents/Resources/Signal.icns"
fi

version=$(sed -n 's/^version = "\(.*\)"/\1/p' apps/desktop/Cargo.toml | head -1)
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Signal</string>
    <key>CFBundleDisplayName</key><string>Signal</string>
    <key>CFBundleIdentifier</key><string>app.fasttrackstudio.signal.mac</string>
    <key>CFBundleExecutable</key><string>Signal</string>
    <key>CFBundleIconFile</key><string>Signal</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>${version:-0.0.1}</string>
    <key>CFBundleVersion</key><string>$(date +%s)</string>
    <key>LSMinimumSystemVersion</key><string>13.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>LSApplicationCategoryType</key><string>public.app-category.music</string>
    <key>NSMicrophoneUsageDescription</key><string>Signal plays your guitar: it listens to your audio interface's inputs.</string>
    <key>NSBluetoothAlwaysUsageDescription</key><string>Signal listens to Bluetooth MIDI footswitches.</string>
</dict>
</plist>
PLIST

# Signed with a stable identity, so what macOS has been granted (the
# microphone, Bluetooth) survives a rebuild: an ad-hoc signature changes with
# every binary and every grant is asked for again. SIGNAL_SIGN_ID picks one;
# else this Mac's Developer ID, else ad-hoc.
sign_id="${SIGNAL_SIGN_ID:-$(security find-identity -v -p codesigning 2>/dev/null | sed -n 's/.*"\(Developer ID Application:[^"]*\)".*/\1/p' | head -1)}"
codesign --force --deep --sign "${sign_id:--}" "$app" >/dev/null 2>&1 \
    || codesign --force --deep --sign - "$app" >/dev/null 2>&1 \
    || echo "codesign failed (the app still runs unsigned locally)" >&2

if [ "${1:-}" = "--install" ]; then
    mkdir -p "$HOME/Applications"
    rm -rf "$HOME/Applications/Signal.app"
    cp -R "$app" "$HOME/Applications/Signal.app"
    echo "$HOME/Applications/Signal.app"
else
    echo "$app"
fi

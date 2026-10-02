#!/usr/bin/env bash
# The Info.plist keys the Signal app needs on iOS, written into a dx-built
# .app — sourced by build-ios.sh (simulator / dev) and deploy-testflight.sh,
# so the two builds cannot drift apart.
#
#   source ios/app-plist.sh && signal_app_plist "$APP"

signal_app_plist() {
    local plist="$1/Info.plist"
    local pb=/usr/libexec/PlistBuddy

    # The UIScene lifecycle — iOS 27 stops an app at launch without it. The
    # delegate is the app's own (src/ios_scene.rs): winit, which Blitz draws
    # into, does not adopt scenes itself yet.
    $pb -c "Delete :UIApplicationSceneManifest" "$plist" 2>/dev/null || true
    $pb \
        -c "Add :UIApplicationSceneManifest dict" \
        -c "Add :UIApplicationSceneManifest:UIApplicationSupportsMultipleScenes bool false" \
        -c "Add :UIApplicationSceneManifest:UISceneConfigurations dict" \
        -c "Add :UIApplicationSceneManifest:UISceneConfigurations:UIWindowSceneSessionRoleApplication array" \
        -c "Add :UIApplicationSceneManifest:UISceneConfigurations:UIWindowSceneSessionRoleApplication:0 dict" \
        -c "Add :UIApplicationSceneManifest:UISceneConfigurations:UIWindowSceneSessionRoleApplication:0:UISceneConfigurationName string Default" \
        -c "Add :UIApplicationSceneManifest:UISceneConfigurations:UIWindowSceneSessionRoleApplication:0:UISceneDelegateClassName string SignalSceneDelegate" \
        "$plist"

    # The app is dark whatever the phone's setting: a light status bar over
    # its background, and a dark launch screen rather than a white flash.
    $pb -c "Delete :UIUserInterfaceStyle" "$plist" 2>/dev/null || true
    $pb -c "Add :UIUserInterfaceStyle string Dark" "$plist"

    # An audio app: the rig keeps sounding with the screen locked or another
    # app in front.
    $pb -c "Delete :UIBackgroundModes" "$plist" 2>/dev/null || true
    $pb -c "Add :UIBackgroundModes array" "$plist"
    $pb -c "Add :UIBackgroundModes:0 string audio" "$plist"

    # An iPhone holds the app sideways only (a portrait layout comes
    # later); an iPad takes every orientation, so it keeps multitasking —
    # App Store Connect refuses a bundle that limits an iPad's orientations
    # without giving multitasking up (90474), and the layout follows the
    # window's size anyway.
    orientations() {
        local key="$1"; shift
        $pb -c "Delete :$key" "$plist" 2>/dev/null || true
        $pb -c "Add :$key array" "$plist"
        local i=0
        for o in "$@"; do
            $pb -c "Add :$key:$i string UIInterfaceOrientation$o" "$plist"
            i=$((i + 1))
        done
    }
    orientations "UISupportedInterfaceOrientations~iphone" LandscapeLeft LandscapeRight
    orientations "UISupportedInterfaceOrientations~ipad" Portrait PortraitUpsideDown LandscapeLeft LandscapeRight
    orientations UISupportedInterfaceOrientations Portrait PortraitUpsideDown LandscapeLeft LandscapeRight
    $pb -c "Delete :UIRequiresFullScreen" "$plist" 2>/dev/null || true
    $pb -c "Add :NSMicrophoneUsageDescription string 'Processes your guitar signal from the connected audio interface or microphone.'" "$plist" 2>/dev/null || true
    # Local network: pack downloads dial the studio engine peer-to-peer
    # (iroh direct paths / LAN WebSocket); without this key iOS silently
    # drops the traffic and the pack host is unreachable on the same Wi-Fi.
    $pb -c "Add :NSLocalNetworkUsageDescription string 'Connects to your studio engine on the local network to stream and download sound packs.'" "$plist" 2>/dev/null || true
    # The Bonjour type the app browses to RAISE the local-network prompt —
    # iroh's raw UDP gets silently filtered instead of prompting without it.
    $pb -c "Add :NSBonjourServices array" "$plist" 2>/dev/null || true
    $pb -c "Add :NSBonjourServices:0 string _fts._tcp" "$plist" 2>/dev/null || true
    # Documents/FastTrackStudio (config, packs) shows in the Files app.
    $pb -c "Add :UIFileSharingEnabled bool true" "$plist" 2>/dev/null || true
    $pb -c "Add :LSSupportsOpeningDocumentsInPlace bool true" "$plist" 2>/dev/null || true
}

# frame's faces (the rig's phone pages) ride in the bundle at frame/; the
# app points SIGNAL_FRAME_DIR there at launch (main.rs). `$1` the .app, `$2`
# frame's examples/plugins (default: the ../frame sibling beside the repo).
signal_app_faces() {
    local app="$1"
    local src="${2:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)/frame/examples/plugins}"
    if [ ! -d "$src/rig-faces" ]; then
        echo "ERROR: no frame faces at $src/rig-faces (check out frame beside the repo)" >&2
        return 1
    fi
    rm -rf "$app/frame"
    mkdir -p "$app/frame"
    # Only the set the rig loads (signal_guitar_ui's rig_faces `SET`): its
    # faces, catalogs and pictures, not the design sources.
    rsync -a --include='*/' --include='*.fm' --include='*.json' --include='*.png' \
        --exclude='*' --prune-empty-dirs "$src/rig-faces/" "$app/frame/rig-faces/"
}

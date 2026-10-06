#!/usr/bin/env bash
# Install Signal for the Mac from a GitHub release (see scripts/mac-release.sh):
# the app into ~/Applications and the release's config as ~/.config/signal.
#
#   bash mac-install.sh [tag]        (default: the newest signal-mac-* release)
#
# The old config is kept beside it (~/.config/signal.backup-<time>), and this
# machine's identity (its iroh key) carries over into the new one.
set -euo pipefail

# The repo is public: plain HTTPS, no gh sign-in needed.
repo=FastTrackStudios/signal
tag="${1:-$(curl -fsSL "https://api.github.com/repos/$repo/releases?per_page=30" |
    grep -o '"tag_name": *"signal-mac-[^"]*"' | head -1 | sed 's/.*"\(signal-mac-[^"]*\)"/\1/')}"
[ -n "$tag" ] || { echo "no signal-mac-* release found" >&2; exit 1; }
echo "installing $tag"

work=$(mktemp -d)
for asset in Signal-macos-arm64.zip signal-config.zip; do
    curl -fL --progress-bar -o "$work/$asset" "https://github.com/$repo/releases/download/$tag/$asset"
done

# Quit the rig (ours — the bundle id, not the Signal messenger).
osascript -e 'quit app id "app.fasttrackstudio.signal.mac"' >/dev/null 2>&1 || true
sleep 2
pkill -f 'Signal.app/Contents/MacOS/signal-desktop' 2>/dev/null || true

cfg="$HOME/.config/signal"
mkdir -p "$HOME/.config"
if [ -d "$cfg" ]; then
    backup="$cfg.backup-$(date +%Y%m%d-%H%M%S)"
    mv "$cfg" "$backup"
    echo "old config kept at $backup"
fi
ditto -x -k "$work/signal-config.zip" "$HOME/.config/"
# This machine stays itself on the network.
if [ -n "${backup:-}" ]; then
    for f in iroh.key iroh-endpoint-id; do
        [ -f "$backup/$f" ] && cp -p "$backup/$f" "$cfg/$f"
    done
fi
# Captures and models are named by absolute path: point them at this home.
find "$cfg" -name '*.styx' -type f -print0 | xargs -0 sed -i '' -E \
    "s#(/Users/[^/\"]+/\\.config|/Volumes/[^/\"]+/config)/signal/#$HOME/.config/signal/#g"

mkdir -p "$HOME/Applications"
rm -rf "$HOME/Applications/Signal.app"
ditto -x -k "$work/Signal-macos-arm64.zip" "$HOME/Applications/"
xattr -dr com.apple.quarantine "$HOME/Applications/Signal.app" 2>/dev/null || true
rm -rf "$work"

open "$HOME/Applications/Signal.app"
echo "Signal $tag installed — ~/Applications/Signal.app"

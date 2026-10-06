#!/usr/bin/env bash
# Publish Signal for the Mac as a GitHub release a laptop installs without
# building: the app (arm64, Developer ID signed) and the rig's whole config
# (profiles, presets, songs, setlists, captures), plus the installer.
#
#   scripts/mac-release.sh signal-mac-2026.10.06
#
# The config is this Mac's ~/.config/signal (or $SIGNAL_CONFIG_SRC), without
# the machine's identity (iroh key) and the migrations' old copies. NOTE: the
# repo is public — so is everything in the release.
#
# On the laptop:
#   curl -fsSL https://github.com/FastTrackStudios/signal/releases/download/<tag>/mac-install.sh | bash -s <tag>
set -euo pipefail
cd "$(dirname "$0")/.."

tag="${1:?usage: scripts/mac-release.sh <tag>  (e.g. signal-mac-$(date +%Y.%m.%d))}"
repo=FastTrackStudios/signal
config="${SIGNAL_CONFIG_SRC:-$HOME/.config/signal}"
dist=target/mac-release
rm -rf "$dist" && mkdir -p "$dist"

CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}" cargo build --profile release-fast -p signal-desktop --features signal-keys-rig
# No baked window size: on the laptop it opens filling the screen.
scripts/mac-app.sh >/dev/null
# ditto keeps the bundle's signature and resource forks intact.
ditto -c -k --keepParent target/Signal.app "$dist/Signal-macos-arm64.zip"

stage=$(mktemp -d)/signal
rsync -a \
    --exclude 'iroh.key' --exclude 'iroh-endpoint-id' \
    --exclude '*.migrated' --exclude 'migrate/' --exclude '.DS_Store' \
    "$config/" "$stage/"
(cd "$(dirname "$stage")" && ditto -c -k --keepParent signal "$OLDPWD/$dist/signal-config.zip")

cp scripts/mac-install.sh "$dist/mac-install.sh"

sha=$(git rev-parse HEAD)
gh release create "$tag" -R "$repo" --target "$sha" \
    --title "Signal for Mac — $tag" \
    --notes "Signal (guitar rig) for Apple-silicon Macs, with the rig's full config: profiles, presets, songs and setlists.

Install on a Mac (Terminal):

\`\`\`sh
curl -fsSL https://github.com/$repo/releases/download/$tag/mac-install.sh | bash -s $tag
\`\`\`

The installer quits Signal, backs up \`~/.config/signal\` (keeping the machine's own identity), installs this config and the app in \`~/Applications\`, and opens it.

Built from $(git rev-parse --short HEAD)." \
    "$dist/Signal-macos-arm64.zip" "$dist/signal-config.zip" "$dist/mac-install.sh"

# Publishing fires release-binaries.yml (cargo-rail's platform builds), which
# is for the v* workspace releases, not this one: cancel what it queued.
# The run appears a few seconds after the release: look for it a while.
for _ in 1 2 3 4 5 6; do
    sleep 5
    ids=$(gh run list -R "$repo" --workflow release-binaries.yml --limit 3 --json databaseId,status \
        -q '.[] | select(.status != "completed") | .databaseId')
    [ -n "$ids" ] || continue
    for id in $ids; do gh run cancel "$id" -R "$repo" >/dev/null 2>&1 || true; done
    break
done
echo "released $tag"

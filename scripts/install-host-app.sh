#!/bin/bash
# Stage fts-clap-host and the plugin bundles it opens onto the INTERNAL disk.
#
# Why any of this is on the internal disk
# ---------------------------------------
# macOS asks for consent before an app touches a removable volume, and it
# attributes that consent to the app's code identity. An ad-hoc-signed binary
# gets a new identity on every rebuild, so the grant never sticks — hence the
# prompt on every single launch. Staging both the host and the bundles it
# reads means the host never touches /Volumes at all, and there is nothing to
# prompt about.
#
# What stays on the external volume: the repo, `target/`, and the whole
# incremental cache — i.e. everything large and everything Cargo writes. What
# comes across is a ~800 KB binary plus a copy of whichever plugin bundles
# you are testing (~35 MB each).
set -euo pipefail

REPO="${FTS_REPO:-/Volumes/build-disk/development/signal}"
APP="${FTS_HOST_APP:-$HOME/Applications/FTSPluginHost.app}"
STAGE="${FTS_HOST_STAGE:-$HOME/Library/Application Support/FTSPluginHost/plugins}"
BIN="$REPO/target/release/fts-clap-host"

[ -x "$BIN" ] || { echo "no host binary at $BIN — run: cargo build --release -p fts-clap-host" >&2; exit 1; }

mkdir -p "$APP/Contents/MacOS"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>FTSPluginHost</string>
<key>CFBundleIdentifier</key><string>com.fasttrackstudio.pluginhost</string>
<key>CFBundleName</key><string>FTSPluginHost</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>1.0</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSEnvironment</key><dict>
  <key>FTS_HOST_TRACE</key><string>1</string>
</dict>
</dict></plist>
PLIST
cp -f "$BIN" "$APP/Contents/MacOS/FTSPluginHost"
codesign --force --sign - --identifier com.fasttrackstudio.pluginhost "$APP" >/dev/null 2>&1

# Stage the requested bundles (default: all of them).
mkdir -p "$STAGE"
names=("$@")
if [ ${#names[@]} -eq 0 ]; then names=(EQ Comp Saturate Delay Reverb); fi
for n in "${names[@]}"; do
    src="$REPO/target/bundled/FTS $n.clap"
    if [ -d "$src" ]; then
        rm -rf "$STAGE/FTS $n.clap"
        cp -R "$src" "$STAGE/FTS $n.clap"
        echo "staged: $STAGE/FTS $n.clap"
    else
        echo "skipped (not built): FTS $n.clap" >&2
    fi
done
echo "installed $APP ($(du -sh "$APP" | cut -f1))"

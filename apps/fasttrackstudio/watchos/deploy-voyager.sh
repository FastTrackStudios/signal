#!/bin/bash
# Build + install + launch the FTS watch app on Cody's Apple Watch.
# Runs in the GUI session so codesign can use the unlocked login keychain.
set -e
WATCH_XCODEBUILD_ID="00008310-000A030111980032"
WATCH_DEVICECTL_ID="93DD4690-2306-56CB-A973-5231372AAED3"
cd ~/fts-watch/watchos
echo "=== building ==="
xcodebuild -project FTSWatch.xcodeproj -scheme FTSWatch \
  -destination "platform=watchOS,id=$WATCH_XCODEBUILD_ID" \
  -allowProvisioningUpdates build 2>&1 | grep -E "error:|BUILD"
APP=$(find ~/Library/Developer/Xcode/DerivedData -path "*FTSWatch*Build/Products/Debug-watchos/FastTrackStudio.app" -not -path "*Index.noindex*" -maxdepth 8 | head -1)
echo "=== installing $APP ==="
xcrun devicectl device install app --device "$WATCH_DEVICECTL_ID" "$APP"
echo "=== launching ==="
xcrun devicectl device process launch --terminate-existing --device "$WATCH_DEVICECTL_ID" app.fasttrackstudio.watch
echo "=== DONE — app relaunched on the watch ==="

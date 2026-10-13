#!/usr/bin/env bash
# Put frame's generated faces on the connected Android device (or emulator),
# where signal-mobile reads them: its external files, `frame/`.
#
#   apps/mobile/scripts/push-faces.sh [FRAME_PLUGINS_DIR]
set -euo pipefail
SRC="${1:-$(cd "$(dirname "$0")/../../../../../frame/examples/plugins" && pwd)}"
DEST=/sdcard/Android/data/app.fasttrackstudio.signal/files/frame
adb shell mkdir -p "$DEST"
adb push --sync "$SRC/rig-faces" "$DEST/" >/dev/null
echo "faces → $DEST/rig-faces"

#!/usr/bin/env bash
# Build, Developer-ID-sign, notarize, and staple a macOS .dmg of the desktop
# app — the downloadable release build for macOS (outside the App Store).
#
# Distinct from deploy-testflight.sh: that uses the "Apple Distribution" cert
# for the App Store; this uses a "Developer ID Application" cert + Apple's
# notary service so Gatekeeper opens it with no warning.
#
# Runs on airlock (headless) with the dedicated keychain:
#   KEYCHAIN=fts-build.keychain KEYCHAIN_PW=fts-build ./ios/deploy-macos.sh
# Needs the App Store Connect key in ~/.appstoreconnect (also used to notarize)
# and setup-keychain.sh already run (WWDR intermediates present).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
cd "$SCRIPT_DIR/.."

# shellcheck disable=SC1090
source "$HOME/.appstoreconnect/config.env"

NIX="${NIX:-}"
if [ -z "$NIX" ]; then
    for c in /run/current-system/sw/bin/nix /nix/var/nix/profiles/default/bin/nix "$(command -v nix 2>/dev/null || true)"; do
        [ -n "$c" ] && [ -x "$c" ] && { NIX="$c"; break; }
    done
fi

KEYCHAIN="${KEYCHAIN:-login.keychain-db}"
KEYCHAIN_PW="${KEYCHAIN_PW:-}"
[ -n "$KEYCHAIN_PW" ] && security unlock-keychain -p "$KEYCHAIN_PW" "$KEYCHAIN"

# ── Developer ID Application identity ────────────────────────────────────────
if ! security find-identity -v -p codesigning "$KEYCHAIN" | grep -q "Developer ID Application"; then
    echo "=== creating Developer ID Application certificate ==="
    eval "$(ruby "$SCRIPT_DIR/mint-developer-id.rb" | grep -E '^DEVID_(KEY|CER)=')"
    openssl x509 -inform DER -in "$DEVID_CER" -out /tmp/fts-devid.pem
    if openssl pkcs12 -help 2>&1 | grep -q -- -legacy; then LEG="-legacy"; else LEG=""; fi
    # shellcheck disable=SC2086
    openssl pkcs12 -export $LEG -inkey "$DEVID_KEY" -in /tmp/fts-devid.pem \
        -name "Developer ID Application" -out /tmp/fts-devid.p12 -passout pass:fts
    security import /tmp/fts-devid.p12 -k "$KEYCHAIN" -P fts -A -T /usr/bin/codesign
    # Developer ID chains through its own intermediate, not WWDR — fetch it.
    curl -fsSL -o /tmp/devidca.cer https://www.apple.com/certificateauthority/DeveloperIDG2CA.cer \
        && security import /tmp/devidca.cer -k "$KEYCHAIN" 2>/dev/null || true
    security set-key-partition-list -S apple-tool:,apple: -s -k "$KEYCHAIN_PW" "$KEYCHAIN" >/dev/null 2>&1 || true
    rm -f /tmp/fts-devid.pem /tmp/fts-devid.p12 /tmp/devidca.cer
fi
SIGN_ID="$(security find-identity -v -p codesigning "$KEYCHAIN" \
    | awk -F'"' '/Developer ID Application/{print $2; exit}')"
[ -n "$SIGN_ID" ] || { echo "ERROR: no Developer ID Application identity." >&2; exit 1; }
echo "=== signing identity: $SIGN_ID ==="

# ── Build (embed the web view so the app serves it on the LAN) ───────────────
APP="$ROOT/target/dx/fasttrackstudio/release/macos/Fasttrackstudio.app"
if [ "${SKIP_BUILD:-}" = "1" ] && [ -d "$APP" ]; then
    echo "SKIP_BUILD=1 — reusing existing app"
else
    echo "=== building macOS app ==="
    # Remove any prior .app so a failed build can't be silently mistaken for a
    # fresh one (dx exits non-zero even on success, so we gate on the .app, not
    # the exit code — but only if it's THIS build's output).
    rm -rf "$APP"
    "$NIX" develop "$ROOT" --accept-flake-config -c bash -c '
        set -euo pipefail
        just web-stage
        cd apps/fasttrackstudio
        dx build --platform macos --release --features embed-web
    ' > /tmp/fts-macos-build.log 2>&1 || true
    tail -3 /tmp/fts-macos-build.log
fi
[ -d "$APP" ] || { echo "ERROR: build produced no app"; tail -30 /tmp/fts-macos-build.log; exit 1; }

# ── Home-screen icon (beta actool — 26.6's is broken on macOS 27) ────────────
ICONS_DIR="$ROOT/apps/fasttrackstudio/ios/Assets.xcassets"
ICON_DEV="${ACTOOL_DEVELOPER_DIR:-$(xcode-select -p)}"
if [ -d "$ICONS_DIR" ] && [ -x "$ICON_DEV/usr/bin/actool" ]; then
    DEVELOPER_DIR="$ICON_DEV" "$ICON_DEV/usr/bin/actool" "$ICONS_DIR" --compile "$APP/Contents/Resources" \
        --platform macosx --minimum-deployment-target 12.0 --app-icon AppIcon \
        --output-partial-info-plist /tmp/fts-macicon.plist >/tmp/fts-macactool.log 2>&1 || true
    /usr/libexec/PlistBuddy -c "Merge /tmp/fts-macicon.plist" "$APP/Contents/Info.plist" 2>/dev/null || true
fi

# ── Hardened runtime entitlements ────────────────────────────────────────────
# allow-jit / allow-unsigned-executable-memory: phon-jit copies its stencils
# into executable memory at runtime — the hardened runtime KILLS that without
# these, so a notarized app would crash on first serialize. audio-input: the
# rig processes an input device. disable-library-validation: load the vendored
# dylibs (NAM, etc.) that aren't Apple-signed.
ENT=/tmp/fts-macos.entitlements
cat > "$ENT" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>com.apple.security.cs.allow-jit</key><true/>
  <key>com.apple.security.cs.allow-unsigned-executable-memory</key><true/>
  <key>com.apple.security.cs.disable-library-validation</key><true/>
  <key>com.apple.security.device.audio-input</key><true/>
</dict></plist>
PLIST

# ── Sign (inside-out): nested code first, then the bundle ────────────────────
echo "=== signing (Developer ID + hardened runtime) ==="
find "$APP" \( -name "*.dylib" -o -name "*.so" -o -name "*.framework" \) -print0 \
    | while IFS= read -r -d '' f; do
        codesign --force --keychain "$KEYCHAIN" --options runtime --timestamp --sign "$SIGN_ID" "$f"
      done
codesign --force --keychain "$KEYCHAIN" --options runtime --timestamp \
    --entitlements "$ENT" --sign "$SIGN_ID" "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"

# ── Package .dmg ─────────────────────────────────────────────────────────────
echo "=== packaging .dmg ==="
BUILD_NO="${BUILD_NO:-$(date +%s)}"
DMG="$ROOT/target/FastTrackStudio-${MARKETING_VER:-0.0.1}-${BUILD_NO}-macos.dmg"
STAGE="$(mktemp -d)"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
hdiutil create -volname "FastTrackStudio" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"
echo "dmg: $DMG"

# ── Notarize + staple ────────────────────────────────────────────────────────
echo "=== notarizing (this waits for Apple) ==="
xcrun notarytool submit "$DMG" \
    --key "$ASC_KEY_PATH" --key-id "$ASC_KEY_ID" --issuer "$ASC_ISSUER_ID" \
    --wait
echo "=== stapling ==="
xcrun stapler staple "$DMG"
xcrun stapler validate "$DMG"
echo "=== DONE — notarized dmg: $DMG ==="

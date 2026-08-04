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

# Which product to build. Defaults to the FastTrackStudio desktop app; for
# Task: DX_PACKAGE=task-app-desktop DX_APP_DIR=apps/task/desktop EMBED_WEB=0 \
#       ICONS_DIR=apps/task/mobile/ios/Assets.xcassets
DX_PACKAGE="${DX_PACKAGE:-fasttrackstudio}"
DX_APP_DIR="${DX_APP_DIR:-apps/fasttrackstudio}"
# Optional Tailwind input (relative to DX_APP_DIR) compiled → assets/tailwind.css
# before the build so the embedded sheet isn't a stale stub (Task desktop).
DX_TAILWIND="${DX_TAILWIND:-}"

# Cross-arch: unset TARGET builds natively for airlock's own arch (Apple
# Silicon). Set TARGET=x86_64-apple-darwin to cross-compile the Intel
# build instead — needs the x86_64-apple-darwin rustc target (nix/modules/
# toolchain.nix). Unlike the plugin bundle (nice-plug-xtask's
# bundle-universal, one lipo'd release for both arches), the app ships as
# two separate arch-specific .dmg downloads — no universal-binary
# machinery for a full dx app bundle exists yet.
TARGET="${TARGET:-}"
case "${TARGET:-$(uname -m)}" in
    aarch64-apple-darwin | arm64) ARCH_TOKEN=aarch64 ;;
    x86_64-apple-darwin | x86_64) ARCH_TOKEN=x86_64 ;;
    *)
        echo "ERROR: unsupported TARGET/arch: ${TARGET:-$(uname -m)}" >&2
        exit 1
        ;;
esac
TARGET_ARG="${TARGET:+--target=$TARGET}"

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
# dx's bundle staging dir under a cross --target isn't something we can
# assume ahead of time, so instead of a fixed glob we look for the most
# recently modified *.app anywhere under target/dx/$DX_PACKAGE and check
# it's actually newer than a marker touched right before the build —
# avoids silently reusing a stale bundle from a previous (different-arch)
# run sharing the same target/ dir.
find_app() {
    find "$ROOT/target/dx/$DX_PACKAGE" -type d -iname '*.app' -exec stat -f '%m %N' {} \; 2>/dev/null \
        | sort -rn | head -1 | cut -d' ' -f2-
}

if [ "${SKIP_BUILD:-}" = "1" ] && [ -n "$(find_app)" ]; then
    echo "SKIP_BUILD=1 — reusing existing app"
else
    echo "=== building macOS app ($ARCH_TOKEN) ==="
    MARKER="$(mktemp)"
    # EMBED_WEB=1 (default) bakes the browser remote into the binary so the app
    # serves it on the LAN. That needs the wasm web build (`just web-stage`),
    # which currently can't run on macOS (a clang-18/clang-21 dylib mismatch in
    # ring's wasm C build). Stage web-dist on Linux and copy it over, OR set
    # EMBED_WEB=0 to ship the native app without the embedded remote.
    if [ "${EMBED_WEB:-1}" = "1" ]; then
        FEATURES="--features embed-web"
        STAGE_WEB='just web-stage'
    else
        FEATURES=""
        STAGE_WEB='echo "EMBED_WEB=0 — skipping web bundle"'
    fi
    "$NIX" develop "$ROOT" --accept-flake-config -c bash -c "
        set -euo pipefail
        cd $ROOT
        $STAGE_WEB
        cd '$ROOT/$DX_APP_DIR'
        # DX_TAILWIND is relative to DX_APP_DIR. Build it from the input's
        # own directory: Tailwind v4's automatic content detection is rooted
        # at the working directory, so the wrong cwd silently drops rules.
        # Matches apps/task/tailwind_build.rs.
        ${DX_TAILWIND:+(cd \"\$(dirname '$DX_TAILWIND')\" && tailwindcss -i \"\$(basename '$DX_TAILWIND')\" -o '$ROOT/$DX_APP_DIR/assets/tailwind.css')}
        dx build --platform macos --release $FEATURES $TARGET_ARG
    " > /tmp/fts-macos-build.log 2>&1 || true
    tail -3 /tmp/fts-macos-build.log
    APP_MTIME="$(find_app | xargs -I{} stat -f '%m' {} 2>/dev/null || echo 0)"
    MARKER_MTIME="$(stat -f '%m' "$MARKER")"
    rm -f "$MARKER"
    if [ -z "$APP_MTIME" ] || [ "$APP_MTIME" -lt "$MARKER_MTIME" ]; then
        echo "ERROR: build produced no new app (found nothing newer than the pre-build marker)"
        tail -30 /tmp/fts-macos-build.log
        exit 1
    fi
fi
APP="$(find_app)"
[ -n "$APP" ] && [ -d "$APP" ] || { echo "ERROR: no app bundle found under target/dx/$DX_PACKAGE"; exit 1; }
echo "=== app bundle: $APP ==="

# ── Home-screen icon (beta actool — 26.6's is broken on macOS 27) ────────────
ICONS_DIR="${ICONS_DIR:-$ROOT/apps/fasttrackstudio/ios/Assets.xcassets}"
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
PRODUCT_NAME="${PRODUCT_NAME:-FastTrackStudio}"
DMG="$ROOT/target/${PRODUCT_NAME}-${MARKETING_VER:-0.0.1}-${BUILD_NO}-${ARCH_TOKEN}-macos.dmg"
STAGE="$(mktemp -d)"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
hdiutil create -volname "$PRODUCT_NAME" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
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

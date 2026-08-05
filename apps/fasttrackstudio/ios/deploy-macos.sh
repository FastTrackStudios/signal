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

# Universal binary: build both Mac architectures and `lipo` them together,
# same idea as nice-plug-xtask's `bundle-universal` for the plugins and
# REAPER's own official "_universal.dmg" — one download, both arches.
# airlock is Apple Silicon with no real Intel hardware; x86_64-apple-darwin
# is a cross-compile (needs that rustc target — nix/modules/toolchain.nix
# — and Apple's SDK linker, which supports it natively on Apple Silicon).
UNIVERSAL_TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)

# ADHOC_SIGN=1: local test build — sign ad-hoc (`-`) instead of Developer ID
# and skip notarization entirely. Apple caps Developer ID certs per account,
# so a throwaway test machine must NOT mint one (it would burn a slot and
# collide with airlock's). Ad-hoc is enough for a binary to RUN locally on
# Apple Silicon, which is all a runnability test needs; it is NOT
# distributable.
if [ "${ADHOC_SIGN:-}" != "1" ]; then
    # shellcheck disable=SC1090
    source "$HOME/.appstoreconnect/config.env"
fi

NIX="${NIX:-}"
if [ -z "$NIX" ]; then
    for c in /run/current-system/sw/bin/nix /nix/var/nix/profiles/default/bin/nix "$(command -v nix 2>/dev/null || true)"; do
        [ -n "$c" ] && [ -x "$c" ] && { NIX="$c"; break; }
    done
fi

KEYCHAIN="${KEYCHAIN:-login.keychain-db}"
KEYCHAIN_PW="${KEYCHAIN_PW:-}"
if [ "${ADHOC_SIGN:-}" = "1" ]; then
    SIGN_ID="-"
    # Ad-hoc + hardened runtime fights the JIT entitlements (phon-jit copies
    # stencils into executable memory), so local test builds skip the
    # hardened runtime — it only matters for notarized distribution anyway.
    RUNTIME_OPTS=()
    echo "=== ADHOC_SIGN=1 — ad-hoc signing, no Developer ID, no notarization ==="
else
RUNTIME_OPTS=(--options runtime --timestamp)
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
fi

# ── Build both arches (embed the web view so the app serves it on the LAN) ──
# dx's bundle staging dir under a cross --target isn't something we can
# assume ahead of time, so instead of a fixed glob we look for *.app
# anywhere under target/dx/$DX_PACKAGE. Freshness is enforced by DELETING
# any existing one before each build (not by mtime — an .app bundle's own
# directory entry only updates when files are added/removed DIRECTLY
# inside it, not when nested Contents/Resources/... files change, so a
# real dx build can finish successfully while leaving the top-level .app
# dir's mtime looking stale).
find_app() {
    find "$ROOT/target/dx/$DX_PACKAGE" -type d -iname '*.app' 2>/dev/null | head -1
}

# A .app DIRECTORY existing is not proof the build worked: dx creates the
# bundle shell before cargo finishes, so a failed compile leaves a hollow
# .app behind and every "did it build?" check that only looks for the
# directory passes — silently packaging an installer with no executable in
# it. Require an actual binary in Contents/MacOS.
app_has_executable() {
    local app="$1"
    [ -n "$app" ] && [ -d "$app" ] || return 1
    [ -n "$(find "$app/Contents/MacOS" -maxdepth 1 -type f 2>/dev/null | head -1)" ]
}

build_one_arch() {
    local target="$1"
    echo "=== building macOS app ($target) ==="
    find "$ROOT/target/dx/$DX_PACKAGE" -type d -iname '*.app' -exec rm -rf {} + 2>/dev/null || true
    # EMBED_WEB=1 (default) bakes the browser remote into the binary so the app
    # serves it on the LAN. That needs the wasm web build (`just web-stage`),
    # which currently can't run on macOS (a clang-18/clang-21 dylib mismatch in
    # ring's wasm C build). Stage web-dist on Linux and copy it over, OR set
    # EMBED_WEB=0 to ship the native app without the embedded remote.
    local features stage_web
    if [ "${EMBED_WEB:-1}" = "1" ]; then
        features="--features embed-web"
        stage_web='just web-stage'
    else
        features=""
        stage_web='echo "EMBED_WEB=0 — skipping web bundle"'
    fi
    "$NIX" develop "$ROOT" --accept-flake-config -c bash -c "
        set -euo pipefail
        cd $ROOT
        $stage_web
        cd '$ROOT/$DX_APP_DIR'
        # DX_TAILWIND is relative to DX_APP_DIR. Build it from the input's
        # own directory: Tailwind v4's automatic content detection is rooted
        # at the working directory, so the wrong cwd silently drops rules.
        # Matches apps/task/tailwind_build.rs.
        ${DX_TAILWIND:+(cd \"\$(dirname '$DX_TAILWIND')\" && tailwindcss -i \"\$(basename '$DX_TAILWIND')\" -o '$ROOT/$DX_APP_DIR/assets/tailwind.css')}
        dx build --platform macos --release $features --target=$target
    " > "/tmp/fts-macos-build-$target.log" 2>&1 || true
    tail -3 "/tmp/fts-macos-build-$target.log"
    if ! app_has_executable "$(find_app)"; then
        echo "ERROR: build produced no usable app for $target (no executable in Contents/MacOS)"
        tail -30 "/tmp/fts-macos-build-$target.log"
        exit 1
    fi
}

if [ "${SKIP_BUILD:-}" = "1" ] && [ -d "$ROOT/target/fts-universal-staging/${UNIVERSAL_TARGETS[0]}" ]; then
    echo "SKIP_BUILD=1 — reusing existing per-arch staged apps"
else
    STAGING="$ROOT/target/fts-universal-staging"
    rm -rf "$STAGING"
    for target in "${UNIVERSAL_TARGETS[@]}"; do
        build_one_arch "$target"
        app="$(find_app)"
        app_has_executable "$app" || { echo "ERROR: no usable app bundle for $target"; exit 1; }
        mkdir -p "$STAGING/$target"
        cp -R "$app" "$STAGING/$target/"
    done
fi

# ── Lipo every Mach-O in the app into one universal bundle ──────────────────
# Base = the first target's staged .app (arbitrary — both should be
# structurally identical, same dx bundling run just a different target).
STAGING="$ROOT/target/fts-universal-staging"
BASE_TARGET="${UNIVERSAL_TARGETS[0]}"
OTHER_TARGET="${UNIVERSAL_TARGETS[1]}"
BASE_APP="$(find "$STAGING/$BASE_TARGET" -maxdepth 1 -iname '*.app' | head -1)"
OTHER_APP="$(find "$STAGING/$OTHER_TARGET" -maxdepth 1 -iname '*.app' | head -1)"
[ -n "$BASE_APP" ] && [ -n "$OTHER_APP" ] || { echo "ERROR: missing staged .app for one or both targets"; exit 1; }

UNIVERSAL_APP="$ROOT/target/$(basename "$BASE_APP")"
rm -rf "$UNIVERSAL_APP"
cp -R "$BASE_APP" "$UNIVERSAL_APP"

echo "=== lipo: merging $BASE_TARGET + $OTHER_TARGET into a universal app ==="
LIPO_WARNINGS=0
while IFS= read -r -d '' f; do
    rel="${f#"$UNIVERSAL_APP"/}"
    other_f="$OTHER_APP/$rel"
    # Not every regular file is Mach-O (resources, plists, etc.) — `lipo
    # -archs` fails cleanly on those, so use it as the Mach-O test too.
    lipo -archs "$f" >/dev/null 2>&1 || continue
    if [ ! -f "$other_f" ]; then
        echo "  WARNING: no $OTHER_TARGET counterpart for $rel — shipping $BASE_TARGET slice only"
        LIPO_WARNINGS=$((LIPO_WARNINGS + 1))
        continue
    fi
    lipo -archs "$other_f" >/dev/null 2>&1 || {
        echo "  WARNING: $OTHER_TARGET counterpart for $rel isn't Mach-O — leaving $BASE_TARGET slice as-is"
        LIPO_WARNINGS=$((LIPO_WARNINGS + 1))
        continue
    }
    tmp="$(mktemp)"
    if lipo -create "$f" "$other_f" -output "$tmp" 2>/tmp/fts-lipo-err.log; then
        mv "$tmp" "$f"
        chmod +x "$f" 2>/dev/null || true
    else
        rm -f "$tmp"
        echo "  WARNING: lipo failed for $rel (likely identical/incompatible slices — see /tmp/fts-lipo-err.log): leaving $BASE_TARGET slice as-is"
        LIPO_WARNINGS=$((LIPO_WARNINGS + 1))
    fi
done < <(find "$UNIVERSAL_APP" -type f -print0)
[ "$LIPO_WARNINGS" -eq 0 ] || echo "=== lipo done with $LIPO_WARNINGS warning(s) — not every binary in the bundle is universal, see above ==="

APP="$UNIVERSAL_APP"

# dx names the bundle by title-casing the crate name (fasttrackstudio ->
# "Fasttrackstudio", task-app-desktop -> "TaskAppDesktop"), which is not the
# product name a user should see in /Applications. Rename before signing —
# CFBundleName is covered by the signature, so doing it afterwards would
# invalidate it.
if [ -n "${APP_BUNDLE_NAME:-}" ] && [ "$(basename "$APP")" != "$APP_BUNDLE_NAME.app" ]; then
    RENAMED="$ROOT/target/$APP_BUNDLE_NAME.app"
    # macOS filesystems are case-INSENSITIVE: when the old and new names
    # differ only in case (Fasttrackstudio -> FastTrackStudio) they are the
    # same path, so `rm -rf "$RENAMED"` would delete the bundle we are about
    # to move. Go via a temp name.
    TMP_APP="$ROOT/target/.rename-$$.app"
    rm -rf "$TMP_APP"
    mv "$APP" "$TMP_APP"
    rm -rf "$RENAMED"
    mv "$TMP_APP" "$RENAMED"
    APP="$RENAMED"
    /usr/libexec/PlistBuddy -c "Set :CFBundleName $APP_BUNDLE_NAME" "$APP/Contents/Info.plist" 2>/dev/null \
        || /usr/libexec/PlistBuddy -c "Add :CFBundleName string $APP_BUNDLE_NAME" "$APP/Contents/Info.plist" 2>/dev/null || true
    echo "=== renamed bundle -> $APP_BUNDLE_NAME.app ==="
fi

echo "=== universal app bundle: $APP ==="
MAIN_EXE="$(find "$APP/Contents/MacOS" -maxdepth 1 -type f | head -1)"
[ -n "$MAIN_EXE" ] && echo "  main executable archs: $(lipo -archs "$MAIN_EXE" 2>/dev/null || echo unknown)"

# ── Home-screen icon (beta actool — 26.6's is broken on macOS 27) ────────────
ICONS_DIR="${ICONS_DIR:-$ROOT/apps/fasttrackstudio/ios/Assets.xcassets}"
ICON_DEV="${ACTOOL_DEVELOPER_DIR:-$(xcode-select -p)}"
if [ -d "$ICONS_DIR" ] && [ -x "$ICON_DEV/usr/bin/actool" ]; then
    DEVELOPER_DIR="$ICON_DEV" "$ICON_DEV/usr/bin/actool" "$ICONS_DIR" --compile "$APP/Contents/Resources" \
        --platform macosx --minimum-deployment-target 12.0 --app-icon AppIcon \
        --output-partial-info-plist /tmp/fts-macicon.plist >/tmp/fts-macactool.log 2>&1 || true
    /usr/libexec/PlistBuddy -c "Merge /tmp/fts-macicon.plist" "$APP/Contents/Info.plist" 2>/dev/null || true
fi

# ── Bundle icon (.icns) ──────────────────────────────────────────────────────
# actool alone is NOT enough here: the shared Assets.xcassets is an iOS icon
# set (a single 1024 image tagged "platform": "ios"), so compiling it with
# --platform macosx emits no Assets.car and no icon at all — which is exactly
# why the app shipped with a blank Finder/Dock icon. dx meanwhile writes
# CFBundleIconFile=icon.icns into Info.plist but never produces the file.
# Build a real multi-resolution .icns from the 1024 master to satisfy it.
ICON_PNG="$(find "$ICONS_DIR" -name 'icon-1024.png' 2>/dev/null | head -1)"
if [ -n "$ICON_PNG" ] && command -v iconutil >/dev/null 2>&1; then
    echo "=== building icon.icns from $(basename "$ICON_PNG") ==="
    ICONSET="$(mktemp -d)/AppIcon.iconset"
    mkdir -p "$ICONSET"
    for sz in 16 32 128 256 512; do
        sips -z "$sz" "$sz" "$ICON_PNG" --out "$ICONSET/icon_${sz}x${sz}.png" >/dev/null 2>&1
        sips -z "$((sz * 2))" "$((sz * 2))" "$ICON_PNG" --out "$ICONSET/icon_${sz}x${sz}@2x.png" >/dev/null 2>&1
    done
    if iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/icon.icns"; then
        # Make sure Info.plist actually points at it (dx usually sets this, but
        # don't depend on it — a missing key means a blank icon again).
        /usr/libexec/PlistBuddy -c "Set :CFBundleIconFile icon.icns" "$APP/Contents/Info.plist" 2>/dev/null \
            || /usr/libexec/PlistBuddy -c "Add :CFBundleIconFile string icon.icns" "$APP/Contents/Info.plist" 2>/dev/null || true
        echo "  icon.icns installed ($(du -h "$APP/Contents/Resources/icon.icns" | cut -f1))"
    else
        echo "  WARNING: iconutil failed — app will have a blank icon"
    fi
    rm -rf "$(dirname "$ICONSET")"
else
    echo "  WARNING: no icon-1024.png under $ICONS_DIR — app will have a blank icon"
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
# --keychain is meaningless for an ad-hoc identity, so only pass it when
# signing for real.
KC_OPTS=()
[ "${ADHOC_SIGN:-}" = "1" ] || KC_OPTS=(--keychain "$KEYCHAIN")
find "$APP" \( -name "*.dylib" -o -name "*.so" -o -name "*.framework" \) -print0 \
    | while IFS= read -r -d '' f; do
        codesign --force "${KC_OPTS[@]}" "${RUNTIME_OPTS[@]}" --sign "$SIGN_ID" "$f"
      done
codesign --force "${KC_OPTS[@]}" "${RUNTIME_OPTS[@]}" \
    --entitlements "$ENT" --sign "$SIGN_ID" "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"

# BUILD_ONLY: stop with a universal, Developer-ID-signed, hardened .app and
# skip the .dmg + notarization. deploy-macos-pkg.sh uses this to get the app
# payload for the .pkg installer (which is notarized once, as a whole, at the
# end — notarizing an intermediate .dmg here would just waste a round trip).
if [ "${BUILD_ONLY:-}" = "1" ]; then
    echo "=== BUILD_ONLY=1 — signed universal app ready, skipping dmg/notarize ==="
    echo "app: $APP"
    exit 0
fi

# ── Package .dmg ─────────────────────────────────────────────────────────────
echo "=== packaging .dmg ==="
BUILD_NO="${BUILD_NO:-$(date +%s)}"
PRODUCT_NAME="${PRODUCT_NAME:-FastTrackStudio}"
DMG="$ROOT/target/${PRODUCT_NAME}-${MARKETING_VER:-0.0.1}-${BUILD_NO}-macos.dmg"
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

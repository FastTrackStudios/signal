#!/usr/bin/env bash
# Build Session.app and its macOS .pkg installer — the native (Blitz) app,
# with its icon, signed, optionally notarized.
#
#   bash apps/desktop/ios/package-session-macos.sh          # signed .pkg
#   NOTARIZE=1 bash apps/desktop/ios/package-session-macos.sh
#   ADHOC_SIGN=1 bash ...                                    # no Apple certs
#
# Unlike its siblings (deploy-macos.sh / deploy-macos-pkg.sh, which build the
# old combined FastTrackStudio app through `nix develop` + `dx`), this builds
# with the HOST toolchain and plain `cargo build --release`. The nix shell's
# rustc differs from the host one and recompiles the whole tree, and the
# native app needs nothing dx bundles: it has no `asset!()`s, and it is
# exactly what `cargo run -p session-desktop` runs.
#
# Output (in target/):
#   Session.app                       the signed app bundle
#   Session-<ver>-macos.pkg           the installer
#
# Installing it:
#   double-click the .pkg, or
#   installer -pkg target/Session-<ver>-macos.pkg -target CurrentUserHomeDirectory
#       (just this user, ~/Applications, no password), or
#   sudo installer -pkg target/Session-<ver>-macos.pkg -target /
#
# Env knobs:
#   MAC_TARGETS="aarch64-apple-darwin x86_64-apple-darwin"
#                     build each and `lipo` them into one universal app. The
#                     default is the host arch only — a universal build is a
#                     second full release build (and needs
#                     `rustup target add x86_64-apple-darwin`).
#   SKIP_BUILD=1      reuse the existing release binaries
#   ADHOC_SIGN=1      ad-hoc sign the app, leave the .pkg unsigned — runs on
#                     this Mac, not distributable
#   NOTARIZE=1        submit the .pkg to Apple's notary service and staple it
#                     (needs ~/.appstoreconnect/config.env), so it opens on
#                     other Macs with no Gatekeeper warning
#   KEYCHAIN=...      where the Developer ID identities live (default: the
#                     login keychain's search list)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
cd "$ROOT"

PRODUCT_NAME=Session
BUNDLE_ID=app.fasttrackstudio.session # Dioxus.toml [bundle] identifier
PACKAGE=session-desktop
ICON_MASTER="$SCRIPT_DIR/Assets.xcassets/AppIcon.appiconset/icon-1024.png"
RESOURCES="$SCRIPT_DIR/installer-resources/session"

HOST_TARGET="$(rustc -vV | awk '/^host:/{print $2}')"
read -r -a TARGETS <<<"${MAC_TARGETS:-$HOST_TARGET}"

# `0.0.2-alpha` -> `0.0.2`: CFBundleShortVersionString must be numeric.
VERSION="$(cargo pkgid -p "$PACKAGE" | sed 's/.*[#@]//')"
SHORT_VERSION="${VERSION%%-*}"
BUILD_NO="${BUILD_NO:-$(date +%Y%m%d%H%M)}"
echo "=== $PRODUCT_NAME $VERSION (build $BUILD_NO) for ${TARGETS[*]} ==="

# ── Build ────────────────────────────────────────────────────────────────────
binary_for() {
    if [ "$1" = "$HOST_TARGET" ] && [ "${#TARGETS[@]}" -eq 1 ]; then
        echo "$ROOT/target/release/$PACKAGE"
    else
        echo "$ROOT/target/$1/release/$PACKAGE"
    fi
}
if [ "${SKIP_BUILD:-}" != "1" ]; then
    for t in "${TARGETS[@]}"; do
        if [ "$t" = "$HOST_TARGET" ] && [ "${#TARGETS[@]}" -eq 1 ]; then
            cargo build --release -p "$PACKAGE"
        else
            cargo build --release -p "$PACKAGE" --target "$t"
        fi
    done
fi
for t in "${TARGETS[@]}"; do
    [ -x "$(binary_for "$t")" ] || { echo "ERROR: no release binary for $t at $(binary_for "$t")" >&2; exit 1; }
done

# ── Assemble the bundle ──────────────────────────────────────────────────────
APP="$ROOT/target/$PRODUCT_NAME.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
EXE="$APP/Contents/MacOS/$PRODUCT_NAME"
if [ "${#TARGETS[@]}" -eq 1 ]; then
    cp "$(binary_for "${TARGETS[0]}")" "$EXE"
else
    bins=()
    for t in "${TARGETS[@]}"; do bins+=("$(binary_for "$t")"); done
    lipo -create "${bins[@]}" -output "$EXE"
fi
chmod 755 "$EXE"
echo "  executable: $(lipo -archs "$EXE"), $(du -h "$EXE" | cut -f1)"

# Anything the binary links outside the OS has to travel inside the bundle.
# Copy each into Contents/Frameworks and point the load command there;
# system libraries (/System, /usr/lib) are always present and stay as-is.
# One level deep is enough for what this app links today; a bundled dylib
# that pulls in another non-system dylib fails loudly below.
bundle_dylibs() {
    local file="$1" dep name
    while IFS= read -r dep; do
        case "$dep" in /System/* | /usr/lib/* | @*) continue ;; esac
        name="$(basename "$dep")"
        mkdir -p "$APP/Contents/Frameworks"
        if [ ! -e "$APP/Contents/Frameworks/$name" ]; then
            cp "$dep" "$APP/Contents/Frameworks/$name"
            chmod 644 "$APP/Contents/Frameworks/$name"
            install_name_tool -id "@rpath/$name" "$APP/Contents/Frameworks/$name"
            echo "  bundled $name"
        fi
        install_name_tool -change "$dep" "@rpath/$name" "$file"
    done < <(otool -L "$file" | tail -n +2 | awk '{print $1}')
}
bundle_dylibs "$EXE"
if [ -d "$APP/Contents/Frameworks" ]; then
    install_name_tool -add_rpath "@executable_path/../Frameworks" "$EXE" 2>/dev/null || true
    for lib in "$APP/Contents/Frameworks"/*.dylib; do
        if otool -L "$lib" | tail -n +2 | awk '{print $1}' | grep -vqE '^(/System/|/usr/lib/|@)'; then
            echo "ERROR: $(basename "$lib") links another non-system library:" >&2
            otool -L "$lib" >&2
            exit 1
        fi
    done
fi

# The icon: macOS does not mask app icons the way iOS does, so the full-bleed
# iOS master goes onto Apple's icon grid first (macos-icon.swift), then into
# every .icns size.
ICONSET="$(mktemp -d)/icon.iconset"
mkdir -p "$ICONSET"
MASTER="$(dirname "$ICONSET")/master.png"
swift "$SCRIPT_DIR/macos-icon.swift" "$ICON_MASTER" "$MASTER"
for sz in 16 32 128 256 512; do
    sips -z "$sz" "$sz" "$MASTER" --out "$ICONSET/icon_${sz}x${sz}.png" >/dev/null
    sips -z "$((sz * 2))" "$((sz * 2))" "$MASTER" --out "$ICONSET/icon_${sz}x${sz}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/icon.icns"
rm -rf "$(dirname "$ICONSET")"

cat >"$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleExecutable</key><string>$PRODUCT_NAME</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleName</key><string>$PRODUCT_NAME</string>
  <key>CFBundleDisplayName</key><string>$PRODUCT_NAME</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$SHORT_VERSION</string>
  <key>CFBundleVersion</key><string>$BUILD_NO</string>
  <key>CFBundleIconFile</key><string>icon</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.music</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <key>NSMicrophoneUsageDescription</key><string>Session records and monitors your audio inputs.</string>
  <key>NSHumanReadableCopyright</key><string>FastTrackStudio — GPL-3.0-or-later</string>
</dict></plist>
PLIST
plutil -lint "$APP/Contents/Info.plist" >/dev/null

# ── Sign ─────────────────────────────────────────────────────────────────────
# Same entitlements as deploy-macos.sh: phon-jit writes its stencils into
# executable memory (the hardened runtime kills that without allow-jit /
# allow-unsigned-executable-memory), audio input for the rig, and library
# validation off for the non-Apple dylibs.
ENT="$(mktemp)"
cat >"$ENT" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>com.apple.security.cs.allow-jit</key><true/>
  <key>com.apple.security.cs.allow-unsigned-executable-memory</key><true/>
  <key>com.apple.security.cs.disable-library-validation</key><true/>
  <key>com.apple.security.device.audio-input</key><true/>
</dict></plist>
PLIST

KC_OPTS=()
[ -n "${KEYCHAIN:-}" ] && KC_OPTS=(--keychain "$KEYCHAIN")
if [ "${ADHOC_SIGN:-}" = "1" ]; then
    SIGN_ID="-"
    RUNTIME_OPTS=() # ad-hoc + hardened runtime fights the JIT entitlements
else
    SIGN_ID="$(security find-identity -v -p codesigning ${KEYCHAIN:+"$KEYCHAIN"} \
        | awk -F'"' '/Developer ID Application/{print $2; exit}')"
    [ -n "$SIGN_ID" ] || { echo "ERROR: no Developer ID Application identity (or set ADHOC_SIGN=1)" >&2; exit 1; }
    RUNTIME_OPTS=(--options runtime --timestamp)
fi
echo "=== signing app: $SIGN_ID ==="
if [ -d "$APP/Contents/Frameworks" ]; then
    for lib in "$APP/Contents/Frameworks"/*; do
        codesign --force ${KC_OPTS[@]+"${KC_OPTS[@]}"} ${RUNTIME_OPTS[@]+"${RUNTIME_OPTS[@]}"} --sign "$SIGN_ID" "$lib"
    done
fi
codesign --force ${KC_OPTS[@]+"${KC_OPTS[@]}"} ${RUNTIME_OPTS[@]+"${RUNTIME_OPTS[@]}"} \
    --entitlements "$ENT" --sign "$SIGN_ID" "$APP"
rm -f "$ENT"
codesign --verify --deep --strict "$APP"
echo "app: $APP"

# ── Package ──────────────────────────────────────────────────────────────────
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/root/Applications" "$WORK/pkgs"
cp -R "$APP" "$WORK/root/Applications/"
# Relocation off: Installer otherwise "upgrades" whatever copy of this bundle
# id it finds first — e.g. a Session.app sitting in target/ — instead of
# installing to /Applications.
pkgbuild --analyze --root "$WORK/root" "$WORK/components.plist" >/dev/null
/usr/libexec/PlistBuddy -c "Add :0:BundleIsRelocatable bool false" "$WORK/components.plist"
pkgbuild --quiet --root "$WORK/root" --component-plist "$WORK/components.plist" \
    --install-location / --identifier "$BUNDLE_ID.app" --version "$VERSION" \
    "$WORK/pkgs/app.pkg"

cat >"$WORK/distribution.xml" <<XML
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
    <title>$PRODUCT_NAME</title>
    <organization>$BUNDLE_ID</organization>
    <!-- All users (admin) or just this user (~/Applications, no password). -->
    <domains enable_currentUserHome="true" enable_localSystem="true"/>
    <options customize="never" require-scripts="false" hostArchitectures="$(lipo -archs "$EXE" | sed 's/ /,/g')"/>
    <background file="background.png" mime-type="image/png" alignment="bottomleft" scaling="none"/>
    <background-darkAqua file="background-dark.png" mime-type="image/png" alignment="bottomleft" scaling="none"/>
    <welcome file="welcome.rtf" mime-type="text/rtf"/>
    <conclusion file="conclusion.rtf" mime-type="text/rtf"/>
    <choices-outline><line choice="choice.app"/></choices-outline>
    <choice id="choice.app" title="$PRODUCT_NAME" start_selected="true">
        <pkg-ref id="$BUNDLE_ID.app"/>
    </choice>
    <pkg-ref id="$BUNDLE_ID.app" version="$VERSION">app.pkg</pkg-ref>
</installer-gui-script>
XML

PKG="$ROOT/target/$PRODUCT_NAME-$VERSION-macos.pkg"
rm -f "$PKG"
if [ "${ADHOC_SIGN:-}" = "1" ]; then
    productbuild --distribution "$WORK/distribution.xml" --package-path "$WORK/pkgs" \
        --resources "$RESOURCES" "$PKG"
else
    INSTALLER_ID="$(security find-identity -v ${KEYCHAIN:+"$KEYCHAIN"} \
        | awk -F'"' '/Developer ID Installer/{print $2; exit}')"
    [ -n "$INSTALLER_ID" ] || { echo "ERROR: no Developer ID Installer identity (or set ADHOC_SIGN=1)" >&2; exit 1; }
    echo "=== signing pkg: $INSTALLER_ID ==="
    productbuild --distribution "$WORK/distribution.xml" --package-path "$WORK/pkgs" \
        --resources "$RESOURCES" ${KC_OPTS[@]+"${KC_OPTS[@]}"} --sign "$INSTALLER_ID" "$PKG"
fi

if [ "${NOTARIZE:-}" = "1" ]; then
    [ "${ADHOC_SIGN:-}" != "1" ] || { echo "ERROR: an ad-hoc build cannot be notarized" >&2; exit 1; }
    # shellcheck disable=SC1090
    source "$HOME/.appstoreconnect/config.env"
    echo "=== notarizing (waits for Apple) ==="
    xcrun notarytool submit "$PKG" --key "$ASC_KEY_PATH" --key-id "$ASC_KEY_ID" \
        --issuer "$ASC_ISSUER_ID" --wait
    xcrun stapler staple "$PKG"
    xcrun stapler validate "$PKG"
fi

echo "=== DONE: $PKG ($(du -h "$PKG" | cut -f1)) ==="
echo "    install (just you): installer -pkg '$PKG' -target CurrentUserHomeDirectory"

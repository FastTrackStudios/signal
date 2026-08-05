#!/usr/bin/env bash
# Build the FastTrackStudio macOS .pkg installer — ONE download that installs
# the desktop app and the whole FTS plugin suite, with per-component checkboxes
# and clean upgrade-in-place on re-run.
#
# Installer tree the user sees (Installer.app "Customize"):
#
#   [x] FastTrackStudio          -> /Applications/FastTrackStudio.app
#   [x] FTS Plugins              (group — toggles every plugin at once)
#       [x] FTS EQ               -> /Library/Audio/Plug-Ins/{CLAP,VST3}/FTS EQ.*
#       [x] FTS Comp
#       ... one line per plugin, each installing BOTH its .clap and .vst3
#
# The plugin list is DISCOVERED from target/bundled at build time, not
# hardcoded — add a plugin to bundler.toml + the Justfile's plugins-bundle
# list and it shows up here automatically.
#
# Updating: component identifiers (app.fasttrackstudio.*) are stable across
# releases, so re-running a newer .pkg upgrades whatever is installed in
# place. Users just download and double-click the new one.
#
# SILENT / UNATTENDED INSTALL (MDM, fleet deploys, CI):
#
#   # everything, no UI:
#   sudo installer -pkg FastTrackStudio-<ver>-macos.pkg -target /
#
#   # pick components — edit the emitted choices file, then:
#   sudo installer -applyChoiceChangesXML FastTrackStudio-<ver>-macos-choices.xml \
#        -pkg FastTrackStudio-<ver>-macos.pkg -target /
#
# A ready-to-edit choices plist is written next to the .pkg listing every
# choice id (choice.app, choice.plugins, choice.plugin.<name>) preselected —
# flip an attributeSetting to 0 to skip that component. Choice ids are stable
# across releases, so a deployment script written once keeps working.
# `installer` is non-interactive by definition; the panes below are only ever
# shown to someone double-clicking in the GUI. Nothing here runs pre/post
# install scripts (require-scripts="false"), so unattended runs can't hang.
#
# Signing needs BOTH Developer ID cert flavours (see mint-developer-id.rb):
# "Developer ID Application" for the payloads (already applied by the two
# build scripts below) and "Developer ID Installer" for the .pkg wrapper.
# The whole .pkg is notarized + stapled once at the end, so Gatekeeper opens
# it offline with no warning.
#
# Runs on airlock (headless), same keychain convention as its siblings:
#   KEYCHAIN=fts-build.keychain KEYCHAIN_PW=fts-build ./ios/deploy-macos-pkg.sh
#
# Env knobs:
#   MARKETING_VER=0.1.0   version baked into the pkg (default: app crate version)
#   SKIP_BUILD=1          reuse existing target/*.app + target/bundled
#   PKG_UNSIGNED=1        skip signing + notarization (local structure testing;
#                         produces an installable-but-unsigned .pkg)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
cd "$ROOT"

PRODUCT_NAME="${PRODUCT_NAME:-FastTrackStudio}"
PKG_ID_PREFIX="${PKG_ID_PREFIX:-app.fasttrackstudio}"

NIX="${NIX:-}"
if [ -z "$NIX" ]; then
    for c in /run/current-system/sw/bin/nix /nix/var/nix/profiles/default/bin/nix "$(command -v nix 2>/dev/null || true)"; do
        [ -n "$c" ] && [ -x "$c" ] && { NIX="$c"; break; }
    done
fi

if [ "${PKG_UNSIGNED:-}" != "1" ]; then
    # shellcheck disable=SC1090
    source "$HOME/.appstoreconnect/config.env"
    KEYCHAIN="${KEYCHAIN:-login.keychain-db}"
    KEYCHAIN_PW="${KEYCHAIN_PW:-}"
    [ -n "$KEYCHAIN_PW" ] && security unlock-keychain -p "$KEYCHAIN_PW" "$KEYCHAIN"
fi

# ── Build the payloads (universal + Developer-ID-signed) ────────────────────
# BUILD_ONLY=1 stops each script right after signing, before it packages and
# notarizes its own artifact — we notarize the .pkg as a whole instead.
if [ "${SKIP_BUILD:-}" = "1" ]; then
    echo "=== SKIP_BUILD=1 — reusing existing target/*.app + target/bundled ==="
else
    echo "=== building the desktop app (universal, signed) ==="
    BUILD_ONLY=1 bash "$SCRIPT_DIR/deploy-macos.sh"
    echo "=== building the plugin suite (universal, signed) ==="
    BUILD_ONLY=1 bash "$SCRIPT_DIR/deploy-macos-plugins.sh"
fi

APP="$(find "$ROOT/target" -maxdepth 1 -iname '*.app' | head -1)"
[ -n "$APP" ] && [ -d "$APP" ] || { echo "ERROR: no .app in $ROOT/target (run without SKIP_BUILD=1)"; exit 1; }
BUNDLED="$ROOT/target/bundled"
[ -d "$BUNDLED" ] || { echo "ERROR: no plugin bundles at $BUNDLED"; exit 1; }
echo "app:     $APP"
echo "plugins: $BUNDLED"

VERSION="${MARKETING_VER:-}"
if [ -z "$VERSION" ]; then
    VERSION="$("$NIX" develop "$ROOT" --accept-flake-config -c cargo pkgid -p fasttrackstudio 2>/dev/null | tail -1 | sed 's/.*[#@]//')"
fi
[ -n "$VERSION" ] || { echo "ERROR: could not determine a version (set MARKETING_VER)"; exit 1; }
echo "version: $VERSION"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
PKGS="$WORK/pkgs"
mkdir -p "$PKGS"

# ── Component 1: the desktop app -> /Applications ───────────────────────────
echo "=== pkgbuild: app ==="
APP_ROOT="$WORK/root-app/Applications"
mkdir -p "$APP_ROOT"
cp -R "$APP" "$APP_ROOT/"
pkgbuild --quiet \
    --root "$WORK/root-app" \
    --install-location / \
    --identifier "$PKG_ID_PREFIX.app" \
    --version "$VERSION" \
    "$PKGS/app.pkg"

# ── Components 2..N: one per plugin (its .clap AND .vst3 together) ──────────
# Discovered from target/bundled so the installer tracks bundler.toml with no
# second list to keep in sync. Each plugin is its own component package, which
# is what makes per-plugin checkboxes possible.
declare -a PLUGIN_IDS=() PLUGIN_TITLES=()

plugin_slug() {
    # "FTS EQ" -> "eq"; "FTS Modulation" -> "modulation"
    printf '%s' "$1" | sed -E 's/^FTS //; s/[^A-Za-z0-9]+/-/g' | tr '[:upper:]' '[:lower:]'
}

# Collect unique plugin display names across both formats. Read newline-
# delimited (bundle names contain SPACES — "FTS Comp" — so an unquoted
# command substitution would split them into garbage, and `declare -A` is out
# because macOS /bin/bash is still 3.2). Sorted for a stable installer list.
declare -a NAMES=()
while IFS= read -r stem; do
    [ -n "$stem" ] && NAMES+=("$stem")
done < <(find "$BUNDLED" -maxdepth 1 \( -iname '*.clap' -o -iname '*.vst3' \) \
             -exec basename {} \; | sed -E 's/\.(clap|vst3)$//' | sort -u)
[ "${#NAMES[@]}" -gt 0 ] || { echo "ERROR: no .clap/.vst3 bundles found in $BUNDLED"; exit 1; }

for name in "${NAMES[@]}"; do
    slug="$(plugin_slug "$name")"
    stage="$WORK/root-plugin-$slug"
    got=0
    if [ -e "$BUNDLED/$name.clap" ]; then
        mkdir -p "$stage/Library/Audio/Plug-Ins/CLAP"
        cp -R "$BUNDLED/$name.clap" "$stage/Library/Audio/Plug-Ins/CLAP/"
        got=1
    fi
    if [ -e "$BUNDLED/$name.vst3" ]; then
        mkdir -p "$stage/Library/Audio/Plug-Ins/VST3"
        cp -R "$BUNDLED/$name.vst3" "$stage/Library/Audio/Plug-Ins/VST3/"
        got=1
    fi
    [ "$got" = 1 ] || continue
    pkgbuild --quiet \
        --root "$stage" \
        --install-location / \
        --identifier "$PKG_ID_PREFIX.plugin.$slug" \
        --version "$VERSION" \
        "$PKGS/plugin-$slug.pkg"
    PLUGIN_IDS+=("$slug")
    PLUGIN_TITLES+=("$name")
    echo "  + $name"
done
echo "=== ${#PLUGIN_IDS[@]} plugin components ==="

# ── Distribution definition (the checkbox tree) ─────────────────────────────
# The "plugins" choice carries no pkg-ref of its own: a choice whose children
# are nested under it in choices-outline renders as a GROUP, so ticking it
# toggles every plugin at once while each child stays individually togglable.
DIST="$WORK/distribution.xml"
{
    cat <<XML
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="2">
    <title>$PRODUCT_NAME</title>
    <organization>$PKG_ID_PREFIX</organization>
    <domains enable_localSystem="true"/>
    <!-- hostArchitectures: the payloads are universal; without this the
         installer refuses to run on one of the two arches. -->
    <options customize="allow" require-scripts="false" hostArchitectures="arm64,x86_64"/>
    <!-- Branding. The background is mostly transparent with the app icon
         parked bottom-left, so Installer's own text stays readable in both
         appearances (a full-bleed dark image makes light-mode body text
         unreadable). Same art for both today; split them if that changes. -->
    <background file="background.png" mime-type="image/png" alignment="bottomleft" scaling="tofit"/>
    <background-darkAqua file="background-dark.png" mime-type="image/png" alignment="bottomleft" scaling="tofit"/>
    <welcome file="welcome.rtf" mime-type="text/rtf"/>
    <conclusion file="conclusion.rtf" mime-type="text/rtf"/>
    <choices-outline>
        <line choice="choice.app"/>
        <line choice="choice.plugins">
XML
    for slug in "${PLUGIN_IDS[@]}"; do
        echo "            <line choice=\"choice.plugin.$slug\"/>"
    done
    cat <<XML
        </line>
    </choices-outline>

    <choice id="choice.app" title="$PRODUCT_NAME" start_selected="true"
            description="The $PRODUCT_NAME desktop application (installs to /Applications).">
        <pkg-ref id="$PKG_ID_PREFIX.app"/>
    </choice>

    <choice id="choice.plugins" title="FTS Plugins" start_selected="true"
            description="The FastTrackStudio plugin suite — CLAP and VST3, installed to /Library/Audio/Plug-Ins. Untick to skip them all, or expand to choose individually."/>
XML
    for i in "${!PLUGIN_IDS[@]}"; do
        slug="${PLUGIN_IDS[$i]}"
        title="${PLUGIN_TITLES[$i]}"
        cat <<XML
    <choice id="choice.plugin.$slug" title="$title" start_selected="true"
            description="$title — CLAP + VST3.">
        <pkg-ref id="$PKG_ID_PREFIX.plugin.$slug"/>
    </choice>
XML
    done
    echo "    <pkg-ref id=\"$PKG_ID_PREFIX.app\" version=\"$VERSION\">app.pkg</pkg-ref>"
    for slug in "${PLUGIN_IDS[@]}"; do
        echo "    <pkg-ref id=\"$PKG_ID_PREFIX.plugin.$slug\" version=\"$VERSION\">plugin-$slug.pkg</pkg-ref>"
    done
    echo "</installer-gui-script>"
} > "$DIST"

# ── Choice-changes template for silent/unattended installs ──────────────────
# Every choice, preselected. Admins copy this next to the .pkg, flip whatever
# they don't want to 0, and feed it to `installer -applyChoiceChangesXML`.
# NB: unlike the GUI, toggling the parent group here does NOT cascade — set
# each plugin child explicitly (that's why they're all listed).
CHOICES="$ROOT/target/${PRODUCT_NAME}-${VERSION}-macos-choices.xml"
{
    cat <<'XML'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<!-- Silent install with these selections:
       sudo installer -applyChoiceChangesXML <this file> -pkg <the .pkg> -target /
     attributeSetting 1 = install, 0 = skip. Omit the file entirely to take
     the defaults (everything). -->
<plist version="1.0">
<array>
XML
    emit_choice() {
        cat <<XML
  <dict>
    <key>choiceIdentifier</key><string>$1</string>
    <key>choiceAttribute</key><string>selected</string>
    <key>attributeSetting</key><integer>1</integer>
  </dict>
XML
    }
    emit_choice "choice.app"
    emit_choice "choice.plugins"
    for slug in "${PLUGIN_IDS[@]}"; do
        emit_choice "choice.plugin.$slug"
    done
    echo "</array>"
    echo "</plist>"
} > "$CHOICES"
echo "choices template: $CHOICES"

# ── productbuild (+ Developer ID Installer signature) ───────────────────────
PKG="$ROOT/target/${PRODUCT_NAME}-${VERSION}-macos.pkg"
rm -f "$PKG"
RESOURCES="$SCRIPT_DIR/installer-resources"

if [ "${PKG_UNSIGNED:-}" = "1" ]; then
    echo "=== PKG_UNSIGNED=1 — building an UNSIGNED .pkg (testing only) ==="
    productbuild --distribution "$DIST" --package-path "$PKGS" --resources "$RESOURCES" "$PKG"
    echo "=== DONE — unsigned pkg: $PKG ==="
    exit 0
fi

# "Developer ID Installer" is a DIFFERENT cert type from the Application one
# codesign uses; mint it on first run. NB: `find-identity -p codesigning`
# would filter installer certs out, so query without a policy.
if ! security find-identity -v "$KEYCHAIN" | grep -q "Developer ID Installer"; then
    echo "=== creating Developer ID Installer certificate ==="
    eval "$(DEVID_CERT_TYPE=DEVELOPER_ID_INSTALLER ruby "$SCRIPT_DIR/mint-developer-id.rb" | grep -E '^DEVID_(KEY|CER)=')"
    openssl x509 -inform DER -in "$DEVID_CER" -out /tmp/fts-devid-installer.pem
    if openssl pkcs12 -help 2>&1 | grep -q -- -legacy; then LEG="-legacy"; else LEG=""; fi
    # shellcheck disable=SC2086
    openssl pkcs12 -export $LEG -inkey "$DEVID_KEY" -in /tmp/fts-devid-installer.pem \
        -name "Developer ID Installer" -out /tmp/fts-devid-installer.p12 -passout pass:fts
    security import /tmp/fts-devid-installer.p12 -k "$KEYCHAIN" -P fts -A -T /usr/bin/productbuild
    curl -fsSL -o /tmp/devidca.cer https://www.apple.com/certificateauthority/DeveloperIDG2CA.cer \
        && security import /tmp/devidca.cer -k "$KEYCHAIN" 2>/dev/null || true
    security set-key-partition-list -S apple-tool:,apple: -s -k "$KEYCHAIN_PW" "$KEYCHAIN" >/dev/null 2>&1 || true
    rm -f /tmp/fts-devid-installer.pem /tmp/fts-devid-installer.p12 /tmp/devidca.cer
fi
INSTALLER_ID="$(security find-identity -v "$KEYCHAIN" \
    | awk -F'"' '/Developer ID Installer/{print $2; exit}')"
[ -n "$INSTALLER_ID" ] || { echo "ERROR: no Developer ID Installer identity." >&2; exit 1; }
echo "=== installer signing identity: $INSTALLER_ID ==="

productbuild --distribution "$DIST" --package-path "$PKGS" --resources "$RESOURCES" \
    --keychain "$KEYCHAIN" --sign "$INSTALLER_ID" "$PKG"

# ── Notarize + staple ───────────────────────────────────────────────────────
echo "=== notarizing (this waits for Apple) ==="
xcrun notarytool submit "$PKG" \
    --key "$ASC_KEY_PATH" --key-id "$ASC_KEY_ID" --issuer "$ASC_ISSUER_ID" \
    --wait
echo "=== stapling ==="
xcrun stapler staple "$PKG"
xcrun stapler validate "$PKG"

echo "=== DONE — notarized pkg: $PKG ==="
echo "    silent install:   sudo installer -pkg '$PKG' -target /"
echo "    choices template: $CHOICES"

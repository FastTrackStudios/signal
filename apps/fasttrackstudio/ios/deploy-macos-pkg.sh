#!/usr/bin/env bash
# Build a macOS .pkg installer. TWO products, one script — each produces its
# OWN separate .pkg with its own component identifiers, so installing or
# updating one never touches the other:
#
#   PKG_PRODUCT=fasttrackstudio  (default)
#       FastTrackStudio-<ver>-macos.pkg — desktop app + the whole plugin suite
#   PKG_PRODUCT=task
#       Task-<ver>-macos.pkg — the Task desktop app on its own (no plugins)
#
# Both are themed with their own app icon and get the same silent-install
# support; the component tree below is the FastTrackStudio one (Task has just
# the single app component, so its installer has nothing to customize).
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
#   # everything, no UI, all users (needs admin):
#   sudo installer -pkg FastTrackStudio-<ver>-macos.pkg -target /
#
#   # ...or just me, NO admin/password at all (-> ~/Applications and
#   # ~/Library/Audio/Plug-Ins):
#   installer -pkg FastTrackStudio-<ver>-macos.pkg -target CurrentUserHomeDirectory
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

# Which product's installer to build. Each ships its OWN .pkg — they are
# separate downloads with separate component identifiers, so installing or
# updating one never touches the other.
#
#   PKG_PRODUCT=fasttrackstudio  (default) desktop app + the 17-plugin suite
#   PKG_PRODUCT=task                       the Task desktop app, no plugins
#
# The per-product build knobs below are the same ones deploy-macos.sh already
# documents for Task; they are exported so it picks them up.
PKG_PRODUCT="${PKG_PRODUCT:-fasttrackstudio}"
case "$PKG_PRODUCT" in
    fasttrackstudio)
        PRODUCT_NAME="${PRODUCT_NAME:-FastTrackStudio}"
        PKG_ID_PREFIX="${PKG_ID_PREFIX:-app.fasttrackstudio}"
        INCLUDE_PLUGINS="${INCLUDE_PLUGINS:-1}"
        ;;
    task)
        PRODUCT_NAME="${PRODUCT_NAME:-Task}"
        # Matches Dioxus.toml's CFBundleIdentifier for the Task desktop app.
        PKG_ID_PREFIX="${PKG_ID_PREFIX:-app.fasttrackstudio.task}"
        INCLUDE_PLUGINS="${INCLUDE_PLUGINS:-0}"
        export DX_PACKAGE="${DX_PACKAGE:-task-app-desktop}"
        export DX_APP_DIR="${DX_APP_DIR:-apps/task/desktop}"
        export ICONS_DIR="${ICONS_DIR:-$ROOT/apps/task/mobile/ios/Assets.xcassets}"
        # Relative to DX_APP_DIR — without it the embedded sheet is a stale stub.
        export DX_TAILWIND="${DX_TAILWIND:-tailwind.css}"
        # Task has no embedded web remote.
        export EMBED_WEB="${EMBED_WEB:-0}"
        ;;
    *)
        echo "ERROR: PKG_PRODUCT must be 'fasttrackstudio' or 'task' (got $PKG_PRODUCT)" >&2
        exit 1
        ;;
esac

# Scratch dir needed before the main WORK below (captures the app build log).
WORK_EARLY="$(mktemp -d)"
trap 'rm -rf "$WORK_EARLY"' EXIT

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
APP="${APP_PATH:-}"
if [ "${SKIP_BUILD:-}" = "1" ]; then
    echo "=== SKIP_BUILD=1 — reusing existing build output ==="
else
    echo "=== building the $PRODUCT_NAME desktop app (universal, signed) ==="
    # Take the app path from deploy-macos.sh's own BUILD_ONLY output rather
    # than globbing target/*.app: with both products built in one tree the
    # glob is ambiguous and would happily package the wrong app.
    BUILD_LOG="$WORK_EARLY/app-build.log"
    set -o pipefail
    BUILD_ONLY=1 bash "$SCRIPT_DIR/deploy-macos.sh" 2>&1 | tee "$BUILD_LOG"
    APP="$(grep '^app: ' "$BUILD_LOG" | tail -1 | sed 's/^app: //')"
    if [ "$INCLUDE_PLUGINS" = "1" ]; then
        echo "=== building the plugin suite (universal, signed) ==="
        BUILD_ONLY=1 bash "$SCRIPT_DIR/deploy-macos-plugins.sh"
    else
        echo "=== $PRODUCT_NAME ships no plugins — skipping the plugin build ==="
    fi
fi

if [ -z "$APP" ]; then
    # SKIP_BUILD path: fall back to a glob, but refuse if it is ambiguous.
    _apps=()
    while IFS= read -r _a; do [ -n "$_a" ] && _apps+=("$_a"); done \
        < <(find "$ROOT/target" -maxdepth 1 -iname '*.app')
    if [ "${#_apps[@]}" -gt 1 ]; then
        echo "ERROR: several .app bundles in $ROOT/target — set APP_PATH to pick one:" >&2
        printf '  %s\n' "${_apps[@]}" >&2
        exit 1
    fi
    APP="${_apps[0]:-}"
fi
[ -n "$APP" ] && [ -d "$APP" ] || { echo "ERROR: no .app found (run without SKIP_BUILD=1, or set APP_PATH)"; exit 1; }
# Never package a hollow bundle — see deploy-macos.sh's app_has_executable.
[ -n "$(find "$APP/Contents/MacOS" -maxdepth 1 -type f 2>/dev/null | head -1)" ] \
    || { echo "ERROR: $APP has no executable in Contents/MacOS — refusing to package a broken app"; exit 1; }
echo "app:     $APP"

BUNDLED="$ROOT/target/bundled"
if [ "$INCLUDE_PLUGINS" = "1" ]; then
    [ -d "$BUNDLED" ] || { echo "ERROR: no plugin bundles at $BUNDLED"; exit 1; }
    echo "plugins: $BUNDLED"
fi

VERSION="${MARKETING_VER:-}"
if [ -z "$VERSION" ]; then
    VERSION="$("$NIX" develop "$ROOT" --accept-flake-config -c cargo pkgid -p fasttrackstudio 2>/dev/null | tail -1 | sed 's/.*[#@]//')"
fi
[ -n "$VERSION" ] || { echo "ERROR: could not determine a version (set MARKETING_VER)"; exit 1; }
echo "version: $VERSION"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK" "$WORK_EARLY"' EXIT
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
if [ "$INCLUDE_PLUGINS" = "1" ]; then
while IFS= read -r stem; do
    [ -n "$stem" ] && NAMES+=("$stem")
done < <(find "$BUNDLED" -maxdepth 1 \( -iname '*.clap' -o -iname '*.vst3' \) \
             -exec basename {} \; | sed -E 's/\.(clap|vst3)$//' | sort -u)
[ "${#NAMES[@]}" -gt 0 ] || { echo "ERROR: no .clap/.vst3 bundles found in $BUNDLED"; exit 1; }
fi

for name in ${NAMES+"${NAMES[@]}"}; do
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
[ "$INCLUDE_PLUGINS" = "1" ] && echo "=== ${#PLUGIN_IDS[@]} plugin components ==="

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
    <!-- Both domains: system-wide (needs admin) OR the user's home with no
         password at all. The payload paths are ./Applications and
         ./Library/..., which Installer re-roots at \$HOME for a user-domain
         install — landing in ~/Applications and ~/Library/Audio/Plug-Ins,
         the same per-user locations fts-installer uses. Scripted:
           sudo installer -pkg X -target /                      (all users)
           installer -pkg X -target CurrentUserHomeDirectory     (just me) -->
    <domains enable_currentUserHome="true" enable_localSystem="true"/>
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
XML
    if [ "$INCLUDE_PLUGINS" = "1" ]; then
        echo '        <line choice="choice.plugins">'
        for slug in ${PLUGIN_IDS+"${PLUGIN_IDS[@]}"}; do
            echo "            <line choice=\"choice.plugin.$slug\"/>"
        done
        echo '        </line>'
    fi
    cat <<XML
    </choices-outline>

    <choice id="choice.app" title="$PRODUCT_NAME" start_selected="true"
            description="The $PRODUCT_NAME desktop application (installs to /Applications).">
        <pkg-ref id="$PKG_ID_PREFIX.app"/>
    </choice>

XML
    if [ "$INCLUDE_PLUGINS" = "1" ]; then
        cat <<XML
    <choice id="choice.plugins" title="FTS Plugins" start_selected="true"
            description="The FastTrackStudio plugin suite — CLAP and VST3. Untick to skip them all, or expand to choose individually."/>
XML
    fi
    for i in ${PLUGIN_IDS+"${!PLUGIN_IDS[@]}"}; do
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
    for slug in ${PLUGIN_IDS+"${PLUGIN_IDS[@]}"}; do
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
    if [ "$INCLUDE_PLUGINS" = "1" ]; then
        emit_choice "choice.plugins"
        for slug in ${PLUGIN_IDS+"${PLUGIN_IDS[@]}"}; do
            emit_choice "choice.plugin.$slug"
        done
    fi
    echo "</array>"
    echo "</plist>"
} > "$CHOICES"
echo "choices template: $CHOICES"

# ── productbuild (+ Developer ID Installer signature) ───────────────────────
PKG="$ROOT/target/${PRODUCT_NAME}-${VERSION}-macos.pkg"
rm -f "$PKG"
RESOURCES="$SCRIPT_DIR/installer-resources/$PKG_PRODUCT"

if [ "${PKG_UNSIGNED:-}" = "1" ]; then
    echo "=== PKG_UNSIGNED=1 — building an UNSIGNED .pkg (testing only) ==="
    productbuild --distribution "$DIST" --package-path "$PKGS" --resources "$RESOURCES" "$PKG"
    echo "=== DONE — unsigned pkg: $PKG ==="
    exit 0
fi

# "Developer ID Installer" is a DIFFERENT cert type from the Application one
# codesign uses; mint it on first run. NB: `find-identity -p codesigning`
# would filter installer certs out, so query without a policy.
# Unlike the Application cert, this one cannot be minted from the App Store
# Connect API — that certificateType simply does not exist there (the enum
# offers DEVELOPER_ID_APPLICATION/_KEXT and the Mac App Store's
# MAC_INSTALLER_DISTRIBUTION, which Gatekeeper rejects for direct
# distribution). It has to be created once by hand in the developer portal.
INSTALLER_ID="$(security find-identity -v "$KEYCHAIN" \
    | awk -F'"' '/Developer ID Installer/{print $2; exit}')"
if [ -z "$INSTALLER_ID" ]; then
    cat >&2 <<'MSG'
ERROR: no "Developer ID Installer" identity in the keychain, and it cannot be
       created automatically (the App Store Connect API has no such
       certificate type).

  Create it once by hand:
    1. https://developer.apple.com/account/resources/certificates/add
       -> "Developer ID Installer"   (needs Account Holder access)
    2. Generate a CSR (Keychain Access > Certificate Assistant), download
       the .cer, and import it + its key into the build keychain.
    3. Verify:  security find-identity -v "$KEYCHAIN" | grep "Developer ID Installer"

  Or, for a local test build that does not need to be distributable:
    PKG_UNSIGNED=1 ...
MSG
    exit 1
fi
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

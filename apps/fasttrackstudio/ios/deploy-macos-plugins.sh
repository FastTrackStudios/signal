#!/usr/bin/env bash
# Build, Developer-ID-sign, and notarize the FTS plugin bundle (CLAP + VST3,
# no AU yet) for macOS — the fts-installer-downloadable
# fts-plugins-v<version>-aarch64-macos.zip release asset.
#
# Distinct from deploy-macos.sh: plugins aren't an .app/.pkg/.dmg, so they
# can't be STAPLED (Apple's stapler only supports those three container
# types) — only signed and submitted for notarization. Gatekeeper falls
# back to an online ticket lookup on first load, which needs network
# access at that point but otherwise behaves the same as a stapled bundle.
#
# Runs on airlock (headless) with the dedicated keychain, same as
# deploy-macos.sh:
#   KEYCHAIN=fts-build.keychain KEYCHAIN_PW=fts-build ./ios/deploy-macos-plugins.sh
# Needs the App Store Connect key in ~/.appstoreconnect (also used to
# notarize) and setup-keychain.sh already run (WWDR intermediates present).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
cd "$ROOT"

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

# ── Developer ID Application identity (same cert deploy-macos.sh uses/mints) ─
if ! security find-identity -v -p codesigning "$KEYCHAIN" | grep -q "Developer ID Application"; then
    echo "=== creating Developer ID Application certificate ==="
    eval "$(ruby "$SCRIPT_DIR/mint-developer-id.rb" | grep -E '^DEVID_(KEY|CER)=')"
    openssl x509 -inform DER -in "$DEVID_CER" -out /tmp/fts-devid.pem
    if openssl pkcs12 -help 2>&1 | grep -q -- -legacy; then LEG="-legacy"; else LEG=""; fi
    # shellcheck disable=SC2086
    openssl pkcs12 -export $LEG -inkey "$DEVID_KEY" -in /tmp/fts-devid.pem \
        -name "Developer ID Application" -out /tmp/fts-devid.p12 -passout pass:fts
    security import /tmp/fts-devid.p12 -k "$KEYCHAIN" -P fts -A -T /usr/bin/codesign
    curl -fsSL -o /tmp/devidca.cer https://www.apple.com/certificateauthority/DeveloperIDG2CA.cer \
        && security import /tmp/devidca.cer -k "$KEYCHAIN" 2>/dev/null || true
    security set-key-partition-list -S apple-tool:,apple: -s -k "$KEYCHAIN_PW" "$KEYCHAIN" >/dev/null 2>&1 || true
    rm -f /tmp/fts-devid.pem /tmp/fts-devid.p12 /tmp/devidca.cer
fi
SIGN_ID="$(security find-identity -v -p codesigning "$KEYCHAIN" \
    | awk -F'"' '/Developer ID Application/{print $2; exit}')"
[ -n "$SIGN_ID" ] || { echo "ERROR: no Developer ID Application identity." >&2; exit 1; }
echo "=== signing identity: $SIGN_ID ==="

# ── Build every plugin bundle (native macOS target, same recipe as Linux) ───
echo "=== building plugin bundles ==="
rm -rf target/bundled
"$NIX" develop "$ROOT" --accept-flake-config -c just plugins-bundle
[ -d target/bundled ] && [ -n "$(ls -A target/bundled 2>/dev/null)" ] || { echo "ERROR: no bundles produced"; exit 1; }
ls target/bundled/

VERSION="$("$NIX" develop "$ROOT" --accept-flake-config -c cargo pkgid -p eq-plugin | sed 's/.*[#@]//')"
echo "=== plugin bundle version: $VERSION ==="

# ── Sign every bundle (inside-out: nested code first, then the bundle) ──────
echo "=== signing (Developer ID + hardened runtime) ==="
for bundle in target/bundled/*; do
    if [ -d "$bundle" ]; then
        find "$bundle" \( -name "*.dylib" -o -name "*.so" -o -name "*.framework" \) -print0 \
            | while IFS= read -r -d '' f; do
                codesign --force --keychain "$KEYCHAIN" --options runtime --timestamp --sign "$SIGN_ID" "$f"
              done
    fi
    codesign --force --keychain "$KEYCHAIN" --options runtime --timestamp --sign "$SIGN_ID" "$bundle"
    codesign --verify --deep --strict --verbose=2 "$bundle"
done

# ── Package + notarize (zip only — stapling needs .app/.pkg/.dmg) ───────────
echo "=== packaging zip ==="
PLAT="aarch64-macos"
ZIP="$ROOT/target/fts-plugins-v${VERSION}-${PLAT}.zip"
rm -f "$ZIP"
ditto -c -k --keepParent target/bundled "$ZIP"
echo "zip: $ZIP"

echo "=== notarizing (this waits for Apple; ticket is NOT stapled to the zip) ==="
xcrun notarytool submit "$ZIP" \
    --key "$ASC_KEY_PATH" --key-id "$ASC_KEY_ID" --issuer "$ASC_ISSUER_ID" \
    --wait
echo "=== DONE — notarized (unstapled) plugin zip: $ZIP ==="

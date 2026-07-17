#!/usr/bin/env bash
# Build a release IPA and upload it to TestFlight via the App Store Connect
# API. Runs in the Mac's GUI login session (codesign needs the unlocked
# login keychain). One-time prerequisites (create in the browser / Xcode):
#   1. App record in App Store Connect for the bundle id (app.fasttrackstudio)
#   2. An "Apple Distribution" certificate in the keychain
#      (Xcode → Settings → Accounts → Manage Certificates → + )
# The App Store Connect API key lives in ~/.appstoreconnect (config.env).
#
#   ./ios/deploy-testflight.sh
#
# Each upload needs a higher build number than the last; we stamp it from
# the current unix time (passed in, since dx/nix can't read the clock).
set -euo pipefail

cd "$(dirname "$0")/.."

TEAM_ID="${TEAM_ID:-28C2G63DA7}"
NIX="${NIX:-/run/current-system/sw/bin/nix}"
BIN_IOS="$HOME/bin-ios"
mkdir -p "$BIN_IOS"
ln -sf /usr/bin/xcrun "$BIN_IOS/xcrun"

# shellcheck disable=SC1090
source "$HOME/.appstoreconnect/config.env"

# Distribution signing identity — create it via the API + import into the
# login keychain if it isn't there yet (no Xcode UI needed).
if ! security find-identity -v -p codesigning | grep -q "Apple Distribution"; then
    echo "=== creating Apple Distribution certificate ==="
    eval "$(ruby "$(dirname "$0")/mint-dist-cert.rb" | grep -E '^DIST_(KEY|CER)=')"
    # Bundle key + cert into a .p12 and import (-A: allow all apps, so
    # codesign uses it without a per-run keychain prompt).
    openssl x509 -inform DER -in "$DIST_CER" -out /tmp/fts-dist.pem
    openssl pkcs12 -export -legacy -inkey "$DIST_KEY" -in /tmp/fts-dist.pem \
        -name "Apple Distribution" -out /tmp/fts-dist.p12 -passout pass:fts
    security import /tmp/fts-dist.p12 -k "$HOME/Library/Keychains/login.keychain-db" \
        -P fts -A -T /usr/bin/codesign
    rm -f /tmp/fts-dist.pem /tmp/fts-dist.p12
fi
SIGN_ID="$(security find-identity -v -p codesigning \
    | awk -F'"' '/Apple Distribution/{print $2; exit}')"
[ -n "$SIGN_ID" ] || { echo "ERROR: distribution identity still missing after import." >&2; exit 1; }
echo "=== distribution identity: $SIGN_ID ==="

echo "=== App Store provisioning profile ==="
PROFILE="$(PROFILE_TYPE=IOS_APP_STORE CERT_TYPE=DISTRIBUTION \
    ruby "$(dirname "$0")/mint-dev-profile.rb" - | awk -F= '/PROFILE_PATH=/{print $2}')"
echo "profile: $PROFILE"

echo "=== building release ==="
# TestFlight needs a RELEASE Xcode/SDK. Point the build at a specific
# Xcode via XCODE_DIR (…/Xcode.app/Contents/Developer) — exporting
# DEVELOPER_DIR overrides both the nix apple-sdk env and xcode-select.
# Unset (default) uses whatever xcode-select points at.
if [ -n "${XCODE_DIR:-}" ]; then
    XCODE_ENV="export DEVELOPER_DIR='$XCODE_DIR'; unset SDKROOT"
    echo "using Xcode: $XCODE_DIR"
else
    XCODE_ENV="unset DEVELOPER_DIR SDKROOT"
fi
APP="$(git rev-parse --show-toplevel)/target/dx/fasttrackstudio/release/ios/Fasttrackstudio.app"
# dx can exit non-zero even on a successful build (and `| tail` + pipefail
# would then abort us), so capture to a log and gate on the .app instead.
"$NIX" develop "$(git rev-parse --show-toplevel)" -c bash -c \
    "$XCODE_ENV; export PATH=$BIN_IOS:\$PATH; \
     dx build --platform ios --device --release --no-default-features --features signal-guitar" \
    > /tmp/fts-build.log 2>&1 || true
tail -2 /tmp/fts-build.log
[ -d "$APP" ] || { echo "ERROR: release build produced no app"; tail -25 /tmp/fts-build.log; exit 1; }

BUNDLE="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP/Info.plist")"

# Info.plist: usage strings + file sharing + versions. The App Store
# requires CFBundleShortVersionString to be 1-3 period-separated integers
# (the crate's "0.0.1-alpha" is rejected); CFBundleVersion just has to climb.
BUILD_NO="${BUILD_NO:-$(date +%s)}"
MARKETING_VER="${MARKETING_VER:-0.0.1}"
# App bundle OS type — App Store requires CFBundlePackageType=APPL (dx omits it).
/usr/libexec/PlistBuddy -c "Set :CFBundlePackageType APPL" "$APP/Info.plist" 2>/dev/null \
    || /usr/libexec/PlistBuddy -c "Add :CFBundlePackageType string APPL" "$APP/Info.plist"
# Launch screen — required because iPad multitasking is implied by the
# orientation set. An empty UILaunchScreen dict = system default (fine).
/usr/libexec/PlistBuddy -c "Add :UILaunchScreen dict" "$APP/Info.plist" 2>/dev/null || true
# Single supported platform — dx leaves both iPhoneOS+iPadOS, which Apple
# rejects (91177). This is an iOS app; keep only iPhoneOS.
/usr/libexec/PlistBuddy -c "Delete :CFBundleSupportedPlatforms" "$APP/Info.plist" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Add :CFBundleSupportedPlatforms array" "$APP/Info.plist"
/usr/libexec/PlistBuddy -c "Add :CFBundleSupportedPlatforms:0 string iPhoneOS" "$APP/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $MARKETING_VER" "$APP/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $BUILD_NO" "$APP/Info.plist"
# Minimum OS — App Store rejects a bundle without it (dx doesn't emit one).
/usr/libexec/PlistBuddy -c "Set :MinimumOSVersion 15.0" "$APP/Info.plist" 2>/dev/null \
    || /usr/libexec/PlistBuddy -c "Add :MinimumOSVersion string 15.0" "$APP/Info.plist"
/usr/libexec/PlistBuddy -c "Add :NSMicrophoneUsageDescription string 'Processes your guitar signal from the connected audio interface or microphone.'" "$APP/Info.plist" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Add :UIFileSharingEnabled bool true" "$APP/Info.plist" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Add :LSSupportsOpeningDocumentsInPlace bool true" "$APP/Info.plist" 2>/dev/null || true
# TestFlight requires ITSAppUsesNonExemptEncryption declared (false = no
# non-standard crypto → no export-compliance docs).
/usr/libexec/PlistBuddy -c "Add :ITSAppUsesNonExemptEncryption bool false" "$APP/Info.plist" 2>/dev/null || true

# SDK build-metadata keys. Xcode's build injects these; dx does NOT, and
# without them App Store Connect can't identify the SDK the binary was built
# against and rejects the upload as "unsupported SDK/Xcode" (error 90534).
# Derive them from the active Xcode/SDK so they stay correct across versions.
# All reads are `|| true`-guarded inside the command sub so a missing key
# can't trip set -e (a bare `var=$(failing)` assignment DOES exit under -e).
DEV="${XCODE_DIR:-$(xcode-select -p)}"
SDK="$DEV/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS.sdk"
SDK_VER="$(/usr/libexec/PlistBuddy -c 'Print :Version' "$SDK/SDKSettings.plist" 2>/dev/null || true)"
SDK_BUILD="$(/usr/libexec/PlistBuddy -c 'Print :ProductBuildVersion' "$DEV/Platforms/iPhoneOS.platform/version.plist" 2>/dev/null || true)"
XCODE_ROOT="${DEV%/Contents/Developer}"
XCODE_BUILD="$(/usr/libexec/PlistBuddy -c 'Print :ProductBuildVersion' "$XCODE_ROOT/Contents/version.plist" 2>/dev/null || true)"
XCODE_VER="$(DEVELOPER_DIR="$DEV" xcodebuild -version 2>/dev/null | awk '/^Xcode/{print $2}' || true)"
# DTXcode = major*100 + minor*10 + patch, 4-digit (26.6 → 2660).
DTXCODE="$(echo "$XCODE_VER" | awk -F. '{printf "%02d%d%d", $1, ($2==""?0:$2), ($3==""?0:$3)}')"
MACOS_BUILD="$(sw_vers -buildVersion)"
add_str() { /usr/libexec/PlistBuddy -c "Add :$1 string $2" "$APP/Info.plist" 2>/dev/null || \
            /usr/libexec/PlistBuddy -c "Set :$1 $2" "$APP/Info.plist" 2>/dev/null || true; }
add_str DTPlatformName iphoneos
add_str DTPlatformVersion "$SDK_VER"
add_str DTPlatformBuild "$SDK_BUILD"
add_str DTSDKName "iphoneos${SDK_VER}"
add_str DTSDKBuild "$SDK_BUILD"
add_str DTXcode "$DTXCODE"
add_str DTXcodeBuild "$XCODE_BUILD"
add_str DTCompiler "com.apple.compilers.llvm.clang.1_0"
add_str BuildMachineOSBuild "$MACOS_BUILD"
echo "=== SDK metadata: iphoneos${SDK_VER} (${SDK_BUILD}), Xcode ${XCODE_VER} (${XCODE_BUILD}) ==="

# Home-screen icon (dx emits none).
ICONS_DIR="$(git rev-parse --show-toplevel)/apps/fasttrackstudio/ios/Assets.xcassets"
if [ -d "$ICONS_DIR" ]; then
    actool "$ICONS_DIR" --compile "$APP" --platform iphoneos \
        --minimum-deployment-target 15.0 --app-icon AppIcon \
        --output-partial-info-plist /tmp/fts-icon.plist >/dev/null 2>&1 \
        && /usr/libexec/PlistBuddy -c "Merge /tmp/fts-icon.plist" "$APP/Info.plist" 2>/dev/null \
        || echo "warn: app-icon compile skipped"
fi
echo "=== app: $APP ($BUNDLE) build $BUILD_NO ==="

echo "=== signing (distribution) ==="
cp "$PROFILE" "$APP/embedded.mobileprovision"
security cms -D -i "$PROFILE" > /tmp/fts-prof.plist
/usr/libexec/PlistBuddy -x -c "Print :Entitlements" /tmp/fts-prof.plist > /tmp/fts-ent.plist
find "$APP" \( -name "*.dylib" -o -name "*.framework" \) -print0 \
    | while IFS= read -r -d '' f; do codesign --force --sign "$SIGN_ID" --timestamp "$f"; done
codesign --force --sign "$SIGN_ID" --entitlements /tmp/fts-ent.plist --timestamp "$APP"
codesign --verify --deep --strict "$APP"

echo "=== packaging IPA ==="
WORK="$(mktemp -d)"
mkdir -p "$WORK/Payload"
cp -R "$APP" "$WORK/Payload/"
IPA="$WORK/FastTrackStudio.ipa"
# ditto (not zip) — preserves the _CodeSignature/CodeResources symlink that
# altool requires; a plain zip breaks it.
( cd "$WORK" && ditto -c -k --sequesterRsrc --keepParent Payload "$IPA" )

echo "=== uploading to TestFlight ==="
xcrun altool --upload-app -t ios -f "$IPA" \
    --apiKey "$ASC_KEY_ID" --apiIssuer "$ASC_ISSUER_ID"
echo "=== DONE — build $BUILD_NO uploaded; it appears in TestFlight after Apple processes it (~5-15 min) ==="

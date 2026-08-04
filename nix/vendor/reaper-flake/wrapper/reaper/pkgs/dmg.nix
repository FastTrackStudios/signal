# Standalone .dmg installer for macOS — no Nix required on the target system.
#
# Produces a "FTS-Reaper.app" that bundles REAPER + SWS + ReaPack.
# On first launch, extensions are automatically installed into the FTS config dir.
{
  lib,
  stdenv,
  reaper,
  sws,
  reapack,
  icon,
  writeScript,
}:

assert stdenv.hostPlatform.isDarwin;

let
  configDir = "$HOME/Library/Application Support/FastTrackStudio/Reaper";

  # Launcher script: sets up extensions on every launch (idempotent), then runs REAPER.
  launcher = writeScript "fts-reaper-launcher" ''
    #!/bin/bash
    set -euo pipefail

    APP_DIR="$(cd "$(dirname "$0")/.." && pwd)"
    REAPER_BIN="$APP_DIR/Resources/REAPER.app/Contents/MacOS/REAPER"
    FTS_EXTENSIONS="$APP_DIR/Resources/FTS/UserPlugins"
    FTS_SCRIPTS="$APP_DIR/Resources/FTS/Scripts"

    CONFIG_DIR="${configDir}"
    mkdir -p "$CONFIG_DIR/UserPlugins" "$CONFIG_DIR/Scripts"

    # Symlink bundled extensions into the config dir
    for dylib in "$FTS_EXTENSIONS"/*.dylib; do
      [ -f "$dylib" ] && ln -sf "$dylib" "$CONFIG_DIR/UserPlugins/"
    done
    for script in "$FTS_SCRIPTS"/*.py; do
      [ -f "$script" ] && ln -sf "$script" "$CONFIG_DIR/Scripts/"
    done

    # Tell REAPER to use our config directory
    exec "$REAPER_BIN" -cfgfile "$CONFIG_DIR/reaper.ini" "$@"
  '';
in
stdenv.mkDerivation {
  pname = "fasttrack-reaper-dmg";
  version = reaper.version;

  dontUnpack = true;
  dontBuild = true;
  dontPatchShebangs = true;

  # hdiutil is a macOS system tool — needs sandbox escape
  __noChroot = true;

  installPhase = ''
    runHook preInstall

    # ── Build the .app bundle ──
    APP="$out/FTS-Reaper.app"
    mkdir -p "$APP/Contents/MacOS"
    mkdir -p "$APP/Contents/Resources/FTS/UserPlugins"
    mkdir -p "$APP/Contents/Resources/FTS/Scripts"

    # Embed REAPER.app inside our wrapper app
    cp -r ${reaper}/Applications/REAPER.app "$APP/Contents/Resources/REAPER.app"

    # Copy extension dylibs
    cp ${sws}/UserPlugins/*.dylib "$APP/Contents/Resources/FTS/UserPlugins/"
    cp ${reapack}/UserPlugins/*.dylib "$APP/Contents/Resources/FTS/UserPlugins/"

    # Copy SWS scripts
    cp ${sws}/Scripts/*.py "$APP/Contents/Resources/FTS/Scripts/" 2>/dev/null || true

    # Install the launcher
    cp ${launcher} "$APP/Contents/MacOS/FTS-Reaper"
    chmod +x "$APP/Contents/MacOS/FTS-Reaper"

    # Use the FTS branded icon
    cp ${icon}/fts-reaper.icns "$APP/Contents/Resources/fts-reaper.icns"

    # Info.plist
    cat > "$APP/Contents/Info.plist" << 'PLIST'
    <?xml version="1.0" encoding="UTF-8"?>
    <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
    <plist version="1.0">
    <dict>
      <key>CFBundleName</key>
      <string>FTS-Reaper</string>
      <key>CFBundleDisplayName</key>
      <string>FTS-Reaper</string>
      <key>CFBundleIdentifier</key>
      <string>com.fasttrackstudio.reaper</string>
      <key>CFBundleVersion</key>
      <string>${reaper.version}</string>
      <key>CFBundleShortVersionString</key>
      <string>${reaper.version}</string>
      <key>CFBundleExecutable</key>
      <string>FTS-Reaper</string>
      <key>CFBundleIconFile</key>
      <string>fts-reaper.icns</string>
      <key>CFBundlePackageType</key>
      <string>APPL</string>
      <key>NSHighResolutionCapable</key>
      <true/>
    </dict>
    </plist>
PLIST

    # ── Create the .dmg ──
    DMG_DIR=$(mktemp -d)
    cp -r "$APP" "$DMG_DIR/"

    # Add a symlink to /Applications for drag-to-install
    ln -s /Applications "$DMG_DIR/Applications"

    /usr/bin/hdiutil create -volname "FTS-Reaper ${reaper.version}" \
      -srcfolder "$DMG_DIR" \
      -ov -format UDZO \
      "$out/FTS-Reaper-${reaper.version}.dmg"

    runHook postInstall
  '';

  meta = {
    description = "FTS-Reaper — standalone macOS installer (.dmg)";
    platforms = [
      "x86_64-darwin"
      "aarch64-darwin"
    ];
  };
}

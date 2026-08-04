# REAPER wrapper module for Lassulus/wrappers.
#
# Provides a portable REAPER installation with SWS and ReaPack extensions
# on both Linux and macOS. Extensions are symlinked into the REAPER config
# directory on first launch via a preHook.
#
# Future: auto-install FTS packages (keyflow, fts-plugins, signal, session, sync).
{
  config,
  lib,
  wlib,
  ...
}:
let
  inherit (config.pkgs) stdenv;

  reaper = config.pkgs.callPackage ./pkgs/reaper.nix {
    jackLibrary = config.pkgs.pipewire.jack or null;
  };

  sws = config.pkgs.callPackage ./pkgs/sws.nix { };
  reapack = config.pkgs.callPackage ./pkgs/reapack.nix { };

  # FTS-specific config directory — isolated from any existing REAPER install.
  # Users can override via the configDir option.
  defaultConfigDir =
    if stdenv.hostPlatform.isDarwin then
      "$HOME/Library/Application Support/FastTrackStudio/Reaper"
    else
      "$HOME/.config/FastTrackStudio/Reaper";

  # Platform-specific extension file suffix
  extSuffix = if stdenv.hostPlatform.isDarwin then "dylib" else "so";

  # Architecture name used in extension filenames
  archName =
    if stdenv.hostPlatform.isAarch64 then
      (if stdenv.hostPlatform.isDarwin then "arm64" else "aarch64")
    else
      "x86_64";
in
{
  _class = "wrapper";

  options = {
    extensions.sws = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Install the SWS extension.";
    };

    extensions.reapack = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Install the ReaPack extension.";
    };

    configDir = lib.mkOption {
      type = lib.types.str;
      default = defaultConfigDir;
      description = "REAPER configuration/resource directory.";
    };
  };

  config = {
    package = reaper;

    meta = {
      maintainers = [ "fasttrackstudio" ];
      platforms = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
    };

    # Set up extensions before REAPER launches.
    # Symlinks are idempotent — safe to run on every invocation.
    preHook =
      let
        swsSetup = lib.optionalString config.extensions.sws ''
          # ── SWS extension ──
          sws_dylib=$(find "${sws}/UserPlugins" -name '*.${extSuffix}' -o -name '*.so' | head -1)
          if [ -n "$sws_dylib" ]; then
            ln -sf "$sws_dylib" "$REAPER_CONFIG/UserPlugins/"
            echo "[reaper-wrapper] SWS linked: $(basename "$sws_dylib")"
          fi
          # SWS Python scripts
          for script in "${sws}/Scripts"/*.py 2>/dev/null; do
            [ -f "$script" ] && ln -sf "$script" "$REAPER_CONFIG/Scripts/"
          done
        '';

        reapackSetup = lib.optionalString config.extensions.reapack ''
          # ── ReaPack extension ──
          reapack_dylib=$(find "${reapack}/UserPlugins" -name '*.${extSuffix}' -o -name '*.so' | head -1)
          if [ -n "$reapack_dylib" ]; then
            ln -sf "$reapack_dylib" "$REAPER_CONFIG/UserPlugins/"
            echo "[reaper-wrapper] ReaPack linked: $(basename "$reapack_dylib")"
          fi
        '';
      in
      ''
        REAPER_CONFIG="${config.configDir}"
        mkdir -p "$REAPER_CONFIG/UserPlugins" "$REAPER_CONFIG/Scripts"
        ${swsSetup}
        ${reapackSetup}
      '';
  };
}

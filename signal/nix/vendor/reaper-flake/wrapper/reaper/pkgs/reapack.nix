# Cross-platform ReaPack extension for REAPER.
#
# Linux: uses the nixpkgs reaper-reapack-extension package.
# macOS: fetches pre-built .dylib from GitHub releases.
{
  lib,
  stdenv,
  fetchurl,
  reaper-reapack-extension ? null,
}:

let
  version = "1.2.6";
in
if stdenv.hostPlatform.isLinux then
  # On Linux, delegate to the nixpkgs package
  reaper-reapack-extension
else
  stdenv.mkDerivation {
    pname = "reaper-reapack-extension";
    inherit version;

    src = fetchurl {
      url =
        let
          arch = if stdenv.hostPlatform.isAarch64 then "arm64" else "x86_64";
        in
        "https://github.com/cfillion/reapack/releases/download/v${version}/reaper_reapack-${arch}.dylib";
      hash =
        {
          aarch64-darwin = "sha256-x2cPOy5AW5A31JsZQaTYw3Yv/zJs7MDFisT67KFx8Hs=";
          x86_64-darwin = "sha256-SLJhl042ZxOEypAqOz1aYUF49Asb63wTjHQUEOpdfZ4=";
        }
        .${stdenv.hostPlatform.system};
    };

    dontUnpack = true;
    dontBuild = true;

    installPhase = ''
      runHook preInstall

      mkdir -p $out/UserPlugins
      cp $src $out/UserPlugins/reaper_reapack-${
        if stdenv.hostPlatform.isAarch64 then "arm64" else "x86_64"
      }.dylib

      runHook postInstall
    '';

    meta = {
      description = "Package manager for REAPER (macOS)";
      homepage = "https://reapack.com/";
      license = lib.licenses.lgpl3Only;
      platforms = [
        "x86_64-darwin"
        "aarch64-darwin"
      ];
    };
  }

# Cross-platform SWS extension for REAPER.
#
# Linux: uses the nixpkgs reaper-sws-extension package.
# macOS: fetches pre-built .dmg from sws-extension.org bleeding edge builds.
{
  lib,
  stdenv,
  fetchurl,
  undmg ? null,
  reaper-sws-extension ? null,
}:

let
  version = "2.14.0.7";
  commit = "9daba634";
in
if stdenv.hostPlatform.isLinux then
  # On Linux, delegate to the nixpkgs package
  reaper-sws-extension
else
  stdenv.mkDerivation {
    pname = "reaper-sws-extension";
    inherit version;

    src = fetchurl {
      url =
        let
          arch = if stdenv.hostPlatform.isAarch64 then "arm64" else "x86_64";
        in
        "https://www.sws-extension.org/download/pre-release/sws-${version}-Darwin-${arch}-${commit}.dmg";
      hash =
        {
          aarch64-darwin = "sha256-AzOLBgh3WECqbFHMTZ4EBGNLpAleXFJT2USzh7pDkQA=";
          x86_64-darwin = "sha256-7wSgiaOcCHpUXBtOBdTTi385M94i8FnWCAp4cN0Rycs=";
        }
        .${stdenv.hostPlatform.system};
    };

    nativeBuildInputs = [ undmg ];

    # undmg extracts everything flat; prevent Nix from cd-ing into Grooves/
    sourceRoot = ".";

    dontBuild = true;

    # The .dmg contains the UserPlugins/ and Scripts/ directories
    installPhase = ''
      runHook preInstall

      mkdir -p $out/UserPlugins $out/Scripts

      # Layout: reaper_sws-{arch}.dylib, sws_python*.py, Grooves/
      find . -name '*.dylib' -exec cp {} $out/UserPlugins/ \;
      find . -name '*.py' -exec cp {} $out/Scripts/ \;
      if [ -d Grooves ]; then
        cp -r Grooves $out/Grooves
      fi

      runHook postInstall
    '';

    meta = {
      description = "SWS extension for REAPER (macOS)";
      homepage = "https://www.sws-extension.org/";
      license = lib.licenses.mit;
      platforms = [
        "x86_64-darwin"
        "aarch64-darwin"
      ];
    };
  }

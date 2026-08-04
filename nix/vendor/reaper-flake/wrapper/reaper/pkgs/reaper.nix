# Cross-platform REAPER derivation (Linux + macOS).
#
# Linux: extracts tarball, patches ELF binaries with autoPatchelf.
# macOS: extracts universal .dmg, installs the .app bundle.
{
  lib,
  stdenv,
  fetchurl,
  makeWrapper,
  # Linux-only
  autoPatchelfHook ? null,
  alsa-lib ? null,
  gtk3 ? null,
  curl,
  lame,
  libxml2,
  ffmpeg,
  xdg-utils ? null,
  xdotool ? null,
  which,
  openssl,
  jackSupport ? stdenv.hostPlatform.isLinux,
  jackLibrary ? null,
  pulseaudioSupport ? stdenv.hostPlatform.isLinux,
  libpulseaudio ? null,
  # macOS-only
  undmg ? null,
}:

let
  version = "7.75";
  versionNoDots = builtins.replaceStrings [ "." ] [ "" ] version;
  majorVersion = lib.versions.major version;

  darwinSrc = fetchurl {
    url = "https://www.reaper.fm/files/${majorVersion}.x/reaper${versionNoDots}_universal.dmg";
    hash = "sha256-rUm/Nyq1QzkxwdEGqc6RGXtpXUcxy1Y4x9YmRL0KElU=";
  };
in
stdenv.mkDerivation {
  pname = "reaper";
  inherit version;

  src =
    {
      x86_64-linux = fetchurl {
        url = "https://www.reaper.fm/files/${majorVersion}.x/reaper${versionNoDots}_linux_x86_64.tar.xz";
        hash = "sha256-BC8W/e1thX1uEKLuPAZ4ALPaCuGfmRVhKmmDvrHEkl4=";
      };
      aarch64-linux = fetchurl {
        url = "https://www.reaper.fm/files/${majorVersion}.x/reaper${versionNoDots}_linux_aarch64.tar.xz";
        hash = "sha256-+93eBKvQYXyvdnWtbVx7eL6QtvuXKKpXtFPJxxdkVYk=";
      };
      x86_64-darwin = darwinSrc;
      aarch64-darwin = darwinSrc;
    }
    .${stdenv.hostPlatform.system} or (throw "Unsupported system: ${stdenv.hostPlatform.system}");

  nativeBuildInputs =
    [ makeWrapper which ]
    ++ lib.optionals stdenv.hostPlatform.isLinux [
      autoPatchelfHook
      xdg-utils
    ]
    ++ lib.optionals stdenv.hostPlatform.isDarwin [
      undmg
    ];

  buildInputs = lib.optionals stdenv.hostPlatform.isLinux [
    (lib.getLib stdenv.cc.cc)
    alsa-lib
    gtk3
  ];

  runtimeDependencies = lib.optionals stdenv.hostPlatform.isLinux (
    lib.optional jackSupport jackLibrary
    ++ lib.optional pulseaudioSupport libpulseaudio
    ++ [ gtk3 ]
  );

  dontBuild = true;
  dontStrip = true;

  # macOS: undmg extracts the .app from the .dmg
  sourceRoot = lib.optionalString stdenv.hostPlatform.isDarwin "REAPER.app";

  installPhase =
    if stdenv.hostPlatform.isDarwin then
      ''
        runHook preInstall

        mkdir -p $out/Applications $out/bin
        cp -r . $out/Applications/REAPER.app

        # Create a bin/reaper wrapper that launches the macOS binary
        makeWrapper $out/Applications/REAPER.app/Contents/MacOS/REAPER $out/bin/reaper

        runHook postInstall
      ''
    else
      ''
        runHook preInstall

        HOME="$out/share" XDG_DATA_HOME="$out/share" ./install-reaper.sh \
          --install $out/opt \
          --integrate-user-desktop
        rm $out/opt/REAPER/uninstall-reaper.sh

        wrapProgram $out/opt/REAPER/reaper \
          --prefix PATH : "${lib.makeBinPath [ xdg-utils ]}" \
          --prefix LD_LIBRARY_PATH : "${
            lib.makeLibraryPath [
              curl
              lame
              libxml2
              ffmpeg
              xdotool
              stdenv.cc.cc
              openssl
            ]
          }"

        mkdir -p $out/bin
        ln -s $out/opt/REAPER/reaper $out/bin/

        substituteInPlace $out/share/applications/cockos-reaper.desktop \
          --replace-fail "Exec=\"$out/opt/REAPER/reaper\"" "Exec=reaper"

        # Fix broken .icons symlink from install script
        rm -f $out/share/.icons

        runHook postInstall
      '';

  meta = {
    description = "Digital audio workstation";
    homepage = "https://www.reaper.fm/";
    sourceProvenance = with lib.sourceTypes; [ binaryNativeCode ];
    license = lib.licenses.unfree;
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ];
  };
}

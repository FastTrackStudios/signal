# Custom REAPER derivation based on nixpkgs, with headless (NOGDK) support.
#
# When `headless = true`, libSwell.so is rebuilt from WDL source with NOGDK=1,
# eliminating all X11/GDK dependencies. REAPER's event loop and timers fire
# via internal polling — no display server required.
{
  lib,
  stdenv,
  fetchurl,
  fetchFromGitHub,
  autoPatchelfHook,
  makeWrapper,

  alsa-lib,
  curl,
  gtk3,
  lame,
  libxml2,
  ffmpeg,
  vlc,
  xdg-utils,
  xdotool,
  which,
  openssl,

  jackSupport ? stdenv.hostPlatform.isLinux,
  jackLibrary,
  pulseaudioSupport ? stdenv.hostPlatform.isLinux,
  libpulseaudio,

  # ── FTS additions ──────────────────────────────────────────
  headless ? false,
}:

let
  version = "7.75";

  url_for_platform =
    arch:
    "https://www.reaper.fm/files/${lib.versions.major version}.x/reaper${
      builtins.replaceStrings [ "." ] [ "" ] version
    }_linux_${arch}.tar.xz";

  # WDL source for building headless libSwell.so
  wdlSrc = fetchFromGitHub {
    owner = "justinfrankel";
    repo = "WDL";
    rev = "afc0f78dca4a1743a948839ac75f4d7059d739c3";
    hash = "sha256-MDc6xh0plccV6uD+J+KuwzQb0L7wN1MPplRS+2CjS8k=";
  };
in
stdenv.mkDerivation (finalAttrs: {
  pname = "reaper" + lib.optionalString headless "-headless";
  inherit version;

  src = fetchurl {
    url = url_for_platform stdenv.hostPlatform.qemuArch;
    hash =
      {
        x86_64-linux = "sha256-BC8W/e1thX1uEKLuPAZ4ALPaCuGfmRVhKmmDvrHEkl4=";
        aarch64-linux = "sha256-+93eBKvQYXyvdnWtbVx7eL6QtvuXKKpXtFPJxxdkVYk=";
      }
      .${stdenv.hostPlatform.system};
  };

  nativeBuildInputs = [
    makeWrapper
    which
    autoPatchelfHook
    xdg-utils
  ];

  buildInputs =
    [
      (lib.getLib stdenv.cc.cc)
      alsa-lib
    ]
    ++ lib.optionals (!headless) [
      gtk3
    ];

  runtimeDependencies =
    lib.optionals (!headless) [
      gtk3
    ]
    ++ lib.optional jackSupport jackLibrary
    ++ lib.optional pulseaudioSupport libpulseaudio;

  # In headless mode, skip unsatisfied deps for plugins we won't use (video needs libGL)
  autoPatchelfIgnoreMissingDeps = lib.optionals headless [
    "libGL.so.1"
    "libgdk-3.so.0"
    "libgobject-2.0.so.0"
    "libglib-2.0.so.0"
  ];

  dontBuild = true;
  dontStrip = true;

  installPhase = ''
    runHook preInstall

    HOME="$out/share" XDG_DATA_HOME="$out/share" ./install-reaper.sh \
      --install $out/opt \
      --integrate-user-desktop
    rm $out/opt/REAPER/uninstall-reaper.sh

    ${lib.optionalString headless ''
      # ── Build headless libSwell.so (NOGDK=1) ───────────────
      # WDL's NOGDK mode provides a headless SWELL implementation that
      # eliminates all X11/GDK dependencies. No source modifications needed.
      echo "Building headless libSwell.so from WDL source (NOGDK=1)..."
      cp -r ${wdlSrc}/WDL /tmp/wdl-build
      chmod -R u+w /tmp/wdl-build
      # WDL bug: swell_oswindow_maximize is declared in swell-internal.h and
      # called from swell-wnd-generic.cpp but has no headless stub.
      # Add the missing no-op stub. (Should be upstreamed to WDL.)
      sed -i '/void swell_oswindow_invalidate/a void swell_oswindow_maximize(HWND hwnd, bool wantmax) { }' \
        /tmp/wdl-build/swell/swell-generic-headless.cpp

      make -C /tmp/wdl-build/swell NOGDK=1 ALLOW_WARNINGS=1 -j$NIX_BUILD_CORES
      cp /tmp/wdl-build/swell/libSwell.so $out/opt/REAPER/libSwell.so
      echo "Headless libSwell.so installed (NOGDK)"
    ''}

    wrapProgram $out/opt/REAPER/reaper \
      --prefix PATH : "${lib.makeBinPath [ xdg-utils ]}" \
      --prefix LD_LIBRARY_PATH : "${
        lib.makeLibraryPath [
          curl
          lame
          libxml2
          ffmpeg
          vlc
          xdotool
          stdenv.cc.cc
          openssl
        ]
      }"

    mkdir $out/bin
    ln -s $out/opt/REAPER/reaper $out/bin/

    substituteInPlace $out/share/applications/cockos-reaper.desktop \
      --replace-fail "Exec=\"$out/opt/REAPER/reaper\"" "Exec=reaper"

    # Fix broken .icons symlink from install script
    rm -f $out/share/.icons

    runHook postInstall
  '';

  meta = {
    description = "Digital audio workstation" + lib.optionalString headless " (headless/NOGDK)";
    homepage = "https://www.reaper.fm/";
    sourceProvenance = with lib.sourceTypes; [ binaryNativeCode ];
    license = lib.licenses.unfree;
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
  };
})

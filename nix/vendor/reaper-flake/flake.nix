{
  description = "reaper-flake — reproducible, declarative REAPER DAW environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";

    # reaper-file: typed Rust mappings for REAPER config / project / kbd files.
    # Used as the canonical reference for INI key names exposed by the
    # programs.reaper NixOS module in modules/reaper/default.nix.
    reaper-file = {
      url = "github:FastTrackStudios/reaper-file";
      flake = false;
    };
  };

  nixConfig = {
    extra-trusted-public-keys = [
      "fasttrackstudio.cachix.org-1:r7v7WXBeSZ7m5meL6w0wttnvsOltRvTpXeVNItcy9f4="
    ];
    extra-substituters = [
      "https://fasttrackstudio.cachix.org"
    ];
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
      reaper-file,
    } @ inputs:
    let
      # ── REAPER environment builder ─────────────────────────────────────────
      mkReaperPackages =
        { pkgs, cfg }:
        let
          # GUI build: standard libSwell with full GDK/X11 support
          reaper = pkgs.callPackage ./pkgs/reaper.nix {
            headless = false;
            jackLibrary = pkgs.pipewire.jack;
            libxml2 = pkgs.libxml2_13; # .so.2 for libSwell (matches nixpkgs)
          };
          # Headless build: custom NOGDK libSwell — no display server required
          reaper-headless-bin = pkgs.callPackage ./pkgs/reaper.nix {
            headless = true;
            jackLibrary = pkgs.pipewire.jack;
            libxml2 = pkgs.libxml2_13; # .so.2 for libSwell (matches nixpkgs)
          };
          sws = pkgs.reaper-sws-extension;
          reapack = pkgs.reaper-reapack-extension;

          graphicsLibs = with pkgs; [
            libx11
            libxi
            libxext
            libxrandr
            libxcursor
            libxinerama
            libxcomposite
            libxdamage
            libxfixes
            libxrender
            libxtst
            libxcb
            gtk3
            gdk-pixbuf
            glib
            pango
            cairo
            atk
            libGL
            libGLU
            libepoxy
            mesa
          ];

          audioLibs =
            with pkgs;
            [ ]
            ++ pkgs.lib.optionals cfg.audio.alsa [ alsa-lib ]
            ++ pkgs.lib.optionals cfg.audio.pipewire [ pipewire wireplumber ]
            ++ pkgs.lib.optionals cfg.audio.jack [ pipewire.jack ]
            ++ pkgs.lib.optionals cfg.audio.pulseaudio [ pulseaudio ];

          codecLibs =
            with pkgs;
            [ ]
            ++ pkgs.lib.optionals cfg.codecs.ffmpeg [ ffmpeg ]
            ++ pkgs.lib.optionals cfg.codecs.lame [ lame ]
            ++ pkgs.lib.optionals cfg.codecs.vorbis [ libvorbis ]
            ++ pkgs.lib.optionals cfg.codecs.ogg [ libogg ]
            ++ pkgs.lib.optionals cfg.codecs.flac [ flac ]
            ++ pkgs.lib.optionals cfg.codecs.opus [ libopus ]
            ++ pkgs.lib.optionals cfg.codecs.sndfile [ libsndfile ];

          pluginPackages =
            with pkgs;
            [ ]
            ++ pkgs.lib.optionals cfg.plugins.lv2 [
              calf
              lsp-plugins
              x42-plugins
              zam-plugins
            ]
            ++ pkgs.lib.optionals cfg.plugins.ladspa [ ladspa-sdk ]
            ++ pkgs.lib.optionals cfg.plugins.clap [ ];

          extensionPackages =
            [ ]
            ++ pkgs.lib.optionals cfg.extensions.sws [ sws ]
            ++ pkgs.lib.optionals cfg.extensions.reapack [ reapack ];

          miscLibs = with pkgs; [
            fontconfig
            freetype
            libsm
            libice
            dbus
            zlib
            stdenv.cc.cc.lib
            # REAPER 7.7x's libSwell dlopen()s libxml2.so.2; the default
            # nixpkgs libxml2 is now soname .so.16, so use libxml2_13 which
            # still ships .so.2 (this is exactly what nixpkgs' reaper does).
            # curl/libxml2_13 are also needed for ReaPack.
            libxml2_13
            curl
          ];

          fhsLibs = graphicsLibs ++ audioLibs ++ codecLibs ++ miscLibs;

          fhsPackages =
            with pkgs;
            [
              reaper
              coreutils
              bash
              procps
              which
              gnugrep
              findutils
            ]
            ++ extensionPackages
            ++ pkgs.lib.optionals cfg.headless.enable [
              xvfb-run
              xdotool
              xauth
              xset
            ]
            ++ pluginPackages;

          extensionSetup =
            let
              swsSetup = pkgs.lib.optionalString cfg.extensions.sws ''
                ln -sf "${sws}/UserPlugins/reaper_sws-x86_64.so" "$REAPER_CONFIG/UserPlugins/"
                ln -sf "${sws}/Scripts/sws_python.py" "$REAPER_CONFIG/Scripts/"
                ln -sf "${sws}/Scripts/sws_python64.py" "$REAPER_CONFIG/Scripts/"
                echo "[reaper-flake] SWS extension linked"
              '';
              reapackSetup = pkgs.lib.optionalString cfg.extensions.reapack ''
                ln -sf "${reapack}/UserPlugins/reaper_reapack-x86_64.so" "$REAPER_CONFIG/UserPlugins/"
                echo "[reaper-flake] ReaPack extension linked"
              '';
            in
            ''
              REAPER_CONFIG="${cfg.reaper.configDir}"
              mkdir -p "$REAPER_CONFIG/UserPlugins" "$REAPER_CONFIG/Scripts"
              ${swsSetup}
              ${reapackSetup}
            '';

          reaper-fhs = pkgs.buildFHSEnv {
            name = "reaper-env";
            targetPkgs = _: fhsPackages;
            multiPkgs = _: fhsLibs;
            profile = ''
              export REAPER_BIN="${reaper}/bin/reaper"
              export REAPER_RESOURCE_DIR="${reaper}/opt/REAPER"
              export REAPER_FLAKE_EXECUTABLE="${reaper}/bin/reaper"
              export REAPER_FLAKE_RESOURCES="${reaper}/opt/REAPER"
              export REAPER_FLAKE_CONFIG="${cfg.reaper.configDir}"
              export LV2_PATH="''${LV2_PATH:+$LV2_PATH:}/usr/lib/lv2"
              export CLAP_PATH="''${CLAP_PATH:+$CLAP_PATH:}/usr/lib/clap"
              export VST_PATH="''${VST_PATH:+$VST_PATH:}/usr/lib/vst"
              export VST3_PATH="''${VST3_PATH:+$VST3_PATH:}/usr/lib/vst3"
              export LADSPA_PATH="''${LADSPA_PATH:+$LADSPA_PATH:}/usr/lib/ladspa"
              export DSSI_PATH="''${DSSI_PATH:+$DSSI_PATH:}/usr/lib/dssi"
            '';
            runScript = "bash";
          };

          reaper-headless-script = pkgs.writeShellScriptBin "reaper-headless" ''
            set -euo pipefail

            REAPER_HOME="${cfg.reaper.configDir}"
            mkdir -p "$REAPER_HOME"
            ${extensionSetup}

            # Write a default reaper.ini if one doesn't exist.
            if [ ! -f "$REAPER_HOME/reaper.ini" ]; then
              cat > "$REAPER_HOME/reaper.ini" << 'INI'
[reaper]
audiodriver=1
lastproject=
undomaxmem=0
[verchk]
audiocloseinactive=0
audioclosestop=0
INI
              echo "[reaper-flake] Default reaper.ini written to $REAPER_HOME"
            fi

            # Start a dedicated PipeWire instance so REAPER has a JACK backend.
            export XDG_RUNTIME_DIR="/tmp/reaper-flake-runtime-$$"
            export PIPEWIRE_RUNTIME_DIR="$XDG_RUNTIME_DIR"
            mkdir -p "$XDG_RUNTIME_DIR"
            pipewire &
            _REAPER_PW_PID=$!
            for i in $(seq 1 20); do
              [ -e "$XDG_RUNTIME_DIR/pipewire-0" ] && break
              sleep 0.1
            done
            if [ -e "$XDG_RUNTIME_DIR/pipewire-0" ]; then
              echo "[reaper-flake] PipeWire started (PID $_REAPER_PW_PID, runtime=$XDG_RUNTIME_DIR)"
              sleep 1
            else
              echo "[reaper-flake] WARNING: PipeWire socket not found after 2s"
            fi

            cleanup() {
              echo "[reaper-flake] Cleaning up..."
              pkill -f "reaper.*-newinst" 2>/dev/null || true
              [ -n "''${_REAPER_PW_PID:-}" ] && kill "$_REAPER_PW_PID" 2>/dev/null || true
            }
            trap cleanup EXIT

            echo "[reaper-flake] Headless mode ready (NOGDK libSwell — no X11 required)"
            export REAPER_FLAKE_EXECUTABLE="${reaper-headless-bin}/bin/reaper"
            export REAPER_FLAKE_RESOURCES="${reaper-headless-bin}/opt/REAPER"

            if [ $# -gt 0 ]; then
              exec "$@"
            else echo "[reaper-flake] No command given — dropping into shell."; exec bash; fi
          '';

          # reaper-headless: FHS sandbox + headless script + NOGDK binary
          reaper-headless-pkg = pkgs.writeShellScriptBin "reaper-headless" ''
            exec ${reaper-fhs}/bin/reaper-env ${reaper-headless-script}/bin/reaper-headless "$@"
          '';

          # reaper-wrapped: FHS sandbox + GUI binary (full GDK/X11)
          reaper-wrapped = pkgs.writeShellScriptBin "reaper" ''
            ${extensionSetup}
            exec ${reaper-fhs}/bin/reaper-env ${reaper}/bin/reaper "$@"
          '';

        in
        {
          inherit
            reaper-fhs
            reaper-headless-bin
            reaper-headless-pkg
            reaper-wrapped
            reaper
            sws
            reapack
            ;
        };

      # ── Preset configs ──────────────────────────────────────────────────────

      defaultConfig = {
        reaper.configDir = "$HOME/.config/REAPER";
        extensions = {
          sws = true;
          reapack = true;
        };
        plugins = {
          lv2 = false;
          vst = false;
          vst3 = false;
          clap = false;
          ladspa = false;
        };
        audio = {
          pipewire = true;
          pulseaudio = true;
          alsa = true;
          jack = true;
        };
        codecs = {
          ffmpeg = false;
          lame = true;
          vorbis = true;
          ogg = true;
          flac = true;
          opus = true;
          sndfile = true;
        };
        headless = {
          enable = true;
          resolution = "1920x1080x24";
          display = ":99";
        };
      };

      presets = {
        ci = defaultConfig // {
          extensions = {
            sws = false;
            reapack = false;
          };
          audio = {
            pipewire = true;
            pulseaudio = false;
            alsa = true;
            jack = true;
          };
          codecs = {
            ffmpeg = false;
            lame = false;
            vorbis = false;
            ogg = false;
            flac = false;
            opus = false;
            sndfile = true;
          };
          headless = {
            enable = true;
            resolution = "1280x720x16";
            display = ":99";
          };
        };

        dev = defaultConfig // {
          plugins = {
            lv2 = true;
            vst = false;
            vst3 = false;
            clap = true;
            ladspa = false;
          };
          codecs = {
            ffmpeg = true;
            lame = true;
            vorbis = true;
            ogg = true;
            flac = true;
            opus = true;
            sndfile = true;
          };
        };

        full = defaultConfig // {
          plugins = {
            lv2 = true;
            vst = false;
            vst3 = false;
            clap = true;
            ladspa = true;
          };
          codecs = {
            ffmpeg = true;
            lame = true;
            vorbis = true;
            ogg = true;
            flac = true;
            opus = true;
            sndfile = true;
          };
          headless = {
            enable = false;
            resolution = "1920x1080x24";
            display = ":99";
          };
        };
      };
    in
    {
      # Expose for consumers
      inherit presets;
      lib.mkReaperPackages = mkReaperPackages;

      # ── NixOS / home-manager modules ─────────────────────────────────────
      nixosModules.default = ./modules/reaper;
      nixosModules.reaper = ./modules/reaper;
    }
    # ── Cross-platform wrapper packages (Linux + macOS) ───────────────────
    // flake-utils.lib.eachSystem [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ] (
      system:
      let
        wrapperPkgs = import nixpkgs {
          inherit system;
          config.allowUnfreePredicate =
            pkg:
            builtins.elem (nixpkgs.lib.getName pkg) [
              "reaper"
            ];
        };

        wrapperReaper = wrapperPkgs.callPackage ./wrapper/reaper/pkgs/reaper.nix {
          jackLibrary = wrapperPkgs.pipewire.jack or null;
        };
        wrapperSws = wrapperPkgs.callPackage ./wrapper/reaper/pkgs/sws.nix { };
        wrapperReapack = wrapperPkgs.callPackage ./wrapper/reaper/pkgs/reapack.nix { };
        wrapperIcon = nixpkgs.lib.optionalAttrs wrapperPkgs.stdenv.hostPlatform.isDarwin (
          wrapperPkgs.callPackage ./wrapper/reaper/pkgs/icon.nix { }
        );
        wrapperDmg = nixpkgs.lib.optionalAttrs wrapperPkgs.stdenv.hostPlatform.isDarwin (
          wrapperPkgs.callPackage ./wrapper/reaper/pkgs/dmg.nix {
            reaper = wrapperReaper;
            sws = wrapperSws;
            reapack = wrapperReapack;
            icon = wrapperIcon;
          }
        );
      in
      {
        wrapperPackages = {
          reaper = wrapperReaper;
          sws = wrapperSws;
          reapack = wrapperReapack;
        } // nixpkgs.lib.optionalAttrs wrapperPkgs.stdenv.hostPlatform.isDarwin {
          icon = wrapperIcon;
          dmg = wrapperDmg;
        };
      }
    )
    # ── Linux-only packages and devShells ─────────────────────────────────
    // flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
          config.allowUnfreePredicate =
            pkg:
            builtins.elem (pkgs.lib.getName pkg) [
              "reaper"
            ];
        };

        defaultPkgs = mkReaperPackages {
          inherit pkgs;
          cfg = defaultConfig;
        };
        devPkgs = mkReaperPackages {
          inherit pkgs;
          cfg = presets.dev;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default;
      in
      {
        packages = {
          default = defaultPkgs.reaper-wrapped;
          reaper = defaultPkgs.reaper-wrapped;
          reaper-headless = defaultPkgs.reaper-headless-pkg;
          reaper-fhs = defaultPkgs.reaper-fhs;
        };

        devShells.default = pkgs.mkShell {
          packages = [
            devPkgs.reaper-headless-pkg
            devPkgs.reaper-wrapped
            devPkgs.reaper-fhs
            rustToolchain
            pkgs.pkg-config
            pkgs.openssl
          ];

          env = {
            REAPER_FLAKE_EXECUTABLE = "${devPkgs.reaper}/bin/reaper";
            REAPER_FLAKE_RESOURCES = "${devPkgs.reaper}/opt/REAPER";
            REAPER_FLAKE_CONFIG = presets.dev.reaper.configDir;
          };

          shellHook = ''
            echo ""
            echo "  reaper-flake dev shell"
            echo "  ────────────────────────────────────────"
            echo "  reaper             — launch REAPER with GUI"
            echo "  reaper-headless    — headless FHS env (CI-ready)"
            echo "  reaper-env         — drop into bare FHS shell"
            echo ""
            echo "  REAPER:  ${devPkgs.reaper}/bin/reaper"
            echo "  SWS:     enabled  |  ReaPack: enabled"
            echo ""
          '';
        };

        checks.reaper-starts = pkgs.runCommand "reaper-starts" { } ''
          test -x ${pkgs.reaper}/opt/REAPER/reaper
          echo "REAPER binary OK" > $out
        '';
      }
    );
}

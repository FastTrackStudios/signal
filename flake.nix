{
  description = "Signal — plugin/signal-chain management for FastTrackStudio";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    devenv.url = "github:cachix/devenv";
    devenv-root = {
      url = "file+file:///dev/null";
      flake = false;
    };
    nix2container.url = "github:nlewo/nix2container";
    nix2container.inputs.nixpkgs.follows = "nixpkgs";
    mk-shell-bin.url = "github:rrbutani/nix-mk-shell-bin";
    fts-flake.url = "github:FastTrackStudios/fts-flake";
  };

  nixConfig = {
    extra-trusted-public-keys = [
      "devenv.cachix.org-1:w1cLUi8dv3hnoSPGAuibQv+f9TZLr6cv/Hm9XgU50cw="
      "fasttrackstudio.cachix.org-1:r7v7WXBeSZ7m5meL6w0wttnvsOltRvTpXeVNItcy9f4="
    ];
    extra-substituters = [
      "https://devenv.cachix.org"
      "https://fasttrackstudio.cachix.org"
    ];
    pure-eval = false;
  };

  outputs = { self, flake-parts, crane, devenv, devenv-root, fts-flake, ... } @inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [ inputs.devenv.flakeModule ];

      systems = [ "x86_64-linux" "x86_64-darwin" "aarch64-darwin" "aarch64-linux" ];

      perSystem = { self', config, pkgs, lib, system, ... }:
        let
          devenvRootFromInput = let
            content = builtins.readFile devenv-root.outPath;
          in pkgs.lib.strings.trim content;
          devenvRoot =
            if devenvRootFromInput != ""
            then devenvRootFromInput
            else builtins.getEnv "PWD";

          # Rust toolchain — same pin as FastTrackStudio
          rustToolchain = pkgs.rust-bin.stable."1.94.0".default.override {
            extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
            targets = [ "wasm32-unknown-unknown" ];
          };

          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

          rev = toString (self.shortRev or self.dirtyShortRev or self.lastModified or "unknown");

          src = lib.fileset.toSource {
            root = ./.;
            fileset = lib.fileset.unions [
              (craneLib.fileset.commonCargoSources ./.)
              (lib.fileset.fileFilter (f: f.hasExt "css") ./.)
              (lib.fileset.fileFilter (f: f.hasExt "ico") ./.)
              (lib.fileset.fileFilter (f: f.hasExt "svg") ./.)
              (lib.fileset.fileFilter (f: f.name == "Dioxus.toml") ./.)
              (lib.fileset.fileFilter (f: f.name == "tailwind-config.js") ./.)
            ];
          };

          buildInputs = (with pkgs; [
            openssl openssl.dev libiconv pkg-config fontconfig freetype cmake python3
          ])
          ++ lib.optionals pkgs.stdenv.isLinux (with pkgs; [
            alsa-lib alsa-lib.dev
            glib gtk3 gdk-pixbuf pango cairo atk
            libsoup_3 webkitgtk_4_1 xdotool
            libx11 libxcursor libxrandr libxi libxcb
            libxkbcommon wayland libGL vulkan-loader
          ])
          ++ lib.optionals pkgs.stdenv.isDarwin (with pkgs; [
            apple-sdk_15
            libiconv
          ]);

          nativeBuildInputs = with pkgs; [
            pkg-config
            rustPlatform.bindgenHook
            dioxus-cli
            wasm-bindgen-cli
            tailwindcss_4
          ];

          commonArgs = {
            inherit src buildInputs nativeBuildInputs;
            strictDeps = true;
            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
            OPENSSL_DIR = "${pkgs.openssl.dev}";
            OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
            CC_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.clang}/bin/clang";
            AR_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.bintools}/bin/llvm-ar";
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          libPath = lib.makeLibraryPath (with pkgs;
            [ fontconfig freetype openssl ]
            ++ lib.optionals pkgs.stdenv.isLinux [
              alsa-lib libGL vulkan-loader gtk3 glib
              gdk-pixbuf pango cairo atk
              libx11 libxcb libxkbcommon wayland
              webkitgtk_4_1 libsoup_3 xdotool
            ]
          );

        in {
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ inputs.rust-overlay.overlays.default ];
            config.allowUnfree = true;
          };

          formatter = pkgs.nixfmt-rfc-style;

          # ============================================================
          # Packages
          # ============================================================
          packages = {
            deps = cargoArtifacts;

            # Signal CLI
            signal-cli = craneLib.buildPackage (commonArgs // {
              pname = "signal-cli";
              version = rev;
              inherit cargoArtifacts;
              cargoExtraArgs = "-p signal-cli";
              doCheck = false;
            });

            # Signal Desktop App
            signal-desktop = craneLib.buildPackage (commonArgs // {
              pname = "signal-desktop";
              version = rev;
              inherit cargoArtifacts;
              buildPhaseCargoCommand = ''
                cd apps/desktop
                dx build --release --platform desktop
              '';
              installPhaseCommand = ''
                mkdir -p $out/Applications $out/bin
                if [ -d "apps/desktop/target/dx/signal-desktop/release/macos" ]; then
                  cp -r apps/desktop/target/dx/signal-desktop/release/macos/*.app $out/Applications/
                  ln -s "$out/Applications/"*.app"/Contents/MacOS/"* $out/bin/signal-desktop
                elif [ -f "target/release/signal-desktop" ]; then
                  cp target/release/signal-desktop $out/bin/
                fi
              '';
              doCheck = false;
            });

            default = self'.packages.signal-desktop;
          };

          # ============================================================
          # Checks
          # ============================================================
          checks = {
            clippy = craneLib.cargoClippy (commonArgs // {
              inherit cargoArtifacts;
              cargoClippyExtraArgs = "--all-targets -- --deny warnings";
            });

            fmt = craneLib.cargoFmt { inherit src; };

            tests = craneLib.cargoNextest (commonArgs // {
              inherit cargoArtifacts;
              partitions = 1;
              partitionType = "count";
            });
          };

          # ============================================================
          # Dev Shell
          # ============================================================
          devenv.shells.default = {
            devenv.root =
              pkgs.lib.mkIf (devenvRoot != "") devenvRoot;

            cachix.pull = [ "fasttrackstudio" ];

            packages = with pkgs; [
              rustToolchain
              dioxus-cli
              wasm-bindgen-cli
              tailwindcss_4
              cargo-watch
              cargo-nextest
              bacon
            ]
            ++ buildInputs
            ++ nativeBuildInputs;

            env = {
              LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
              OPENSSL_DIR = "${pkgs.openssl.dev}";
              OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
              CC_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.clang}/bin/clang";
              AR_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.bintools}/bin/llvm-ar";
              RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
            }
            // lib.optionalAttrs pkgs.stdenv.isLinux {
              LD_LIBRARY_PATH = libPath;
              XDG_DATA_DIRS = "${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}";
            }
            // lib.optionalAttrs pkgs.stdenv.isDarwin {
              DYLD_LIBRARY_PATH = libPath;
            };

            scripts = {
              signal-build.exec = "cargo build --workspace";
              signal-build.description = "Build entire workspace";

              signal-check.exec = "cargo clippy --workspace -- -D warnings";
              signal-check.description = "Run clippy with warnings-as-errors";

              signal-test.exec = "cargo nextest run --workspace";
              signal-test.description = "Run all unit tests";
            };

            enterShell = ''
              [ -f .env ] && { set -a; source .env; set +a; }
              echo ""
              echo "  Signal dev shell (devenv)"
              echo "  ────────────────────────────────────────"
              echo "  signal-build  — cargo build --workspace"
              echo "  signal-check  — clippy (warnings-as-errors)"
              echo "  signal-test   — cargo nextest run --workspace"
              echo ""
              echo "  Rust: $(rustc --version)"
              echo "  dx:   $(dx --version 2>/dev/null || echo 'not available')"
              echo ""
            '';
          };
        };
    };
}

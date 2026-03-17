{
  description = "FastTrackStudio - DAW control system with roam RPC";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    devenv.url = "github:cachix/devenv";
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
  };

  outputs = { self, flake-parts, crane, devenv, nix2container, fts-flake, ... } @inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [ inputs.devenv.flakeModule ];

      systems = [ "x86_64-linux" "x86_64-darwin" "aarch64-darwin" "aarch64-linux" ];

      perSystem = { self', config, pkgs, lib, system, ... }:
        let
          # Rust toolchain with WASM support
          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
            targets = [ "wasm32-unknown-unknown" ];
          };

          # Crane lib configured with our toolchain
          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

          # Version info
          rev = toString (self.shortRev or self.dirtyShortRev or self.lastModified or "unknown");

          # Source filtering — include Rust sources plus Dioxus assets
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

          # Build dependencies
          buildInputs = (with pkgs; [
            openssl openssl.dev libiconv pkg-config fontconfig freetype cmake python3
          ])
          ++ lib.optionals pkgs.stdenv.isLinux (with pkgs; [
            alsa-lib alsa-lib.dev
            glib gtk3 libsoup_3 webkitgtk_4_1 xdotool
            xorg.libX11 xorg.libXcursor xorg.libXrandr xorg.libXi xorg.libxcb
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

          # Common args for all crane builds
          commonArgs = {
            inherit src buildInputs nativeBuildInputs;
            strictDeps = true;
            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
            OPENSSL_DIR = "${pkgs.openssl.dev}";
            OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
            CC_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.clang}/bin/clang";
            AR_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.bintools}/bin/llvm-ar";
          };

          # Build workspace dependencies (cached)
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          # Library path for runtime
          libPath = lib.makeLibraryPath (with pkgs;
            [ fontconfig freetype openssl ]
            ++ lib.optionals pkgs.stdenv.isLinux [
              alsa-lib libGL vulkan-loader gtk3 glib
              xorg.libX11 xorg.libxcb libxkbcommon wayland
              webkitgtk_4_1 libsoup_3
            ]
          );

        in {
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ inputs.rust-overlay.overlays.default ];
          };

          formatter = pkgs.nixfmt-rfc-style;

          # ============================================================
          # Packages
          # ============================================================
          packages = {
            deps = cargoArtifacts;

            # Host runtime library
            host-runtime = craneLib.buildPackage (commonArgs // {
              pname = "host-runtime";
              version = rev;
              inherit cargoArtifacts;
              cargoExtraArgs = "-p host-runtime";
              doCheck = false;
            });

            # Test extension host (uses daw-standalone)
            test-extension = craneLib.buildPackage (commonArgs // {
              pname = "test-extension";
              version = rev;
              inherit cargoArtifacts;
              cargoExtraArgs = "-p test-extension";
              doCheck = false;
            });

            # REAPER extension host (uses daw-reaper)
            reaper-extension = craneLib.buildPackage (commonArgs // {
              pname = "reaper-extension";
              version = rev;
              inherit cargoArtifacts;
              cargoExtraArgs = "-p reaper-extension";
              doCheck = false;
            });

            # FTS Control Web App (WASM)
            fts-control-web = craneLib.buildPackage (commonArgs // {
              pname = "fts-control-web";
              version = rev;
              inherit cargoArtifacts;
              buildPhaseCargoCommand = ''
                cd apps/fts-control/web
                dx build --release --platform web
              '';
              installPhaseCommand = ''
                mkdir -p $out/www
                cp -r apps/fts-control/web/target/dx/fts-control-web/release/web/* $out/www/
              '';
              doCheck = false;
            });

            # FTS Control Desktop App
            fts-control-desktop = craneLib.buildPackage (commonArgs // {
              pname = "fts-control-desktop";
              version = rev;
              inherit cargoArtifacts;
              buildPhaseCargoCommand = ''
                cd apps/fts-control/desktop
                dx build --release --platform desktop
              '';
              installPhaseCommand = ''
                mkdir -p $out/Applications $out/bin
                if [ -d "apps/fts-control/desktop/target/dx/fts-control-desktop/release/macos" ]; then
                  cp -r apps/fts-control/desktop/target/dx/fts-control-desktop/release/macos/*.app $out/Applications/
                  ln -s "$out/Applications/"*.app"/Contents/MacOS/"* $out/bin/fts-control-desktop
                elif [ -f "target/release/fts-control-desktop" ]; then
                  cp target/release/fts-control-desktop $out/bin/
                fi
              '';
              doCheck = false;
            });

            # FastTrackStudio Desktop App
            fasttrackstudio-desktop = craneLib.buildPackage (commonArgs // {
              pname = "fasttrackstudio-desktop";
              version = rev;
              inherit cargoArtifacts;
              buildPhaseCargoCommand = ''
                cd apps/fasttrackstudio/desktop
                dx build --release --platform desktop
              '';
              installPhaseCommand = ''
                mkdir -p $out/Applications $out/bin
                if [ -d "apps/fasttrackstudio/desktop/target/dx/fasttrackstudio-desktop/release/macos" ]; then
                  cp -r apps/fasttrackstudio/desktop/target/dx/fasttrackstudio-desktop/release/macos/*.app $out/Applications/
                  ln -s "$out/Applications/"*.app"/Contents/MacOS/"* $out/bin/fasttrackstudio-desktop
                elif [ -f "target/release/fasttrackstudio-desktop" ]; then
                  cp target/release/fasttrackstudio-desktop $out/bin/
                fi
              '';
              doCheck = false;
            });

            # DAW Standalone cell
            daw-standalone = craneLib.buildPackage (commonArgs // {
              pname = "daw-standalone";
              version = rev;
              inherit cargoArtifacts;
              cargoExtraArgs = "-p daw-standalone";
              doCheck = false;
            });

            # Session cell
            session = craneLib.buildPackage (commonArgs // {
              pname = "session";
              version = rev;
              inherit cargoArtifacts;
              cargoExtraArgs = "-p session";
              doCheck = false;
            });

            # Gateway WebSocket cell
            gateway-ws = craneLib.buildPackage (commonArgs // {
              pname = "gateway-ws";
              version = rev;
              inherit cargoArtifacts;
              cargoExtraArgs = "-p gateway-ws";
              doCheck = false;
            });

            # FTS Installer Desktop App
            fts-installer = craneLib.buildPackage (commonArgs // {
              pname = "fts-installer";
              version = rev;
              inherit cargoArtifacts;
              buildPhaseCargoCommand = ''
                cd apps/installer
                dx build --release --platform desktop
              '';
              installPhaseCommand = ''
                mkdir -p $out/Applications $out/bin
                if [ -d "apps/installer/target/dx/fts-installer/release/macos" ]; then
                  cp -r apps/installer/target/dx/fts-installer/release/macos/*.app $out/Applications/
                  ln -s "$out/Applications/"*.app"/Contents/MacOS/"* $out/bin/fts-installer
                elif [ -f "target/release/fts-installer" ]; then
                  cp target/release/fts-installer $out/bin/
                fi
              '';
              doCheck = false;
            });

            default = self'.packages.test-extension;
          };

          # ============================================================
          # Apps — installer DMG
          # ============================================================
          apps.create-installer-dmg = {
            type = "app";
            program = let
              script = pkgs.writeShellScript "create-installer-dmg" ''
                set -euo pipefail

                INSTALLER_APP="${self'.packages.fts-installer}/Applications"
                FTS_CONTROL_APP="${self'.packages.fts-control-desktop}/Applications"
                OUTPUT="FastTrackStudio-Installer-${rev}.dmg"
                STAGING=$(mktemp -d)

                echo "Assembling installer DMG..."

                # Copy installer app
                if [ -d "$INSTALLER_APP" ]; then
                  cp -r "$INSTALLER_APP"/*.app "$STAGING/"
                else
                  echo "ERROR: FTS Installer app not found at $INSTALLER_APP"
                  exit 1
                fi

                # Copy FTS Control app (installer will copy this into the install dir)
                if [ -d "$FTS_CONTROL_APP" ]; then
                  cp -r "$FTS_CONTROL_APP"/*.app "$STAGING/"
                else
                  echo "WARNING: FTS Control app not found, installer will skip that step"
                fi

                # Create the DMG
                hdiutil create -volname "FastTrackStudio" \
                  -srcfolder "$STAGING" \
                  -ov -format UDZO \
                  "$OUTPUT"

                rm -rf "$STAGING"
                echo "Created $OUTPUT ($(du -h "$OUTPUT" | cut -f1))"
              '';
            in "${script}";
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
          # Dev Shells (devenv-powered)
          # ============================================================
          devenv.shells.default = let
            ftsPkgs = lib.optionalAttrs pkgs.stdenv.isLinux (
              fts-flake.lib.mkFtsPackages {
                inherit pkgs;
                cfg = fts-flake.presets.dev;
              }
            );
          in {
            cachix.pull = [ "fasttrackstudio" ];

            packages = with pkgs; [
              rustToolchain
              dioxus-cli
              wasm-bindgen-cli
              tailwindcss_4
              cargo-watch
              cargo-nextest
              bacon
              nodejs_22
            ]
            ++ buildInputs
            ++ nativeBuildInputs
            ++ lib.optionals pkgs.stdenv.isLinux [
              ftsPkgs.fts-test
              ftsPkgs.fts-gui
              ftsPkgs.reaper-fhs
            ];

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
              FTS_REAPER_EXECUTABLE = "${ftsPkgs.reaper}/bin/reaper";
              FTS_REAPER_RESOURCES = "${ftsPkgs.reaper}/opt/REAPER";
            }
            // lib.optionalAttrs pkgs.stdenv.isDarwin {
              DYLD_LIBRARY_PATH = libPath;
            };

            scripts = {
              fts-check.exec = "cargo clippy --workspace -- -D warnings";
              fts-check.description = "Run clippy with warnings-as-errors";

              fts-fmt.exec = "cargo fmt --all -- --check";
              fts-fmt.description = "Check formatting";

              fts-test.exec = "cargo nextest run --workspace";
              fts-test.description = "Run all unit tests";

              fts-build.exec = "cargo build --workspace";
              fts-build.description = "Build entire workspace";
            }
            // lib.optionalAttrs pkgs.stdenv.isLinux {
              fts-smoke.exec = ''
                fts-test bash -c '
                  "$FTS_REAPER_EXECUTABLE" -newinst -nosplash -ignoreerrors &
                  RPID=$!
                  sleep 3
                  if kill -0 $RPID 2>/dev/null; then
                    echo "REAPER running (PID $RPID) — smoke test passed"
                    kill $RPID
                  else
                    echo "REAPER failed to start"
                    exit 1
                  fi
                '
              '';
              fts-smoke.description = "REAPER headless smoke test";

              fts-reaper-test.exec = ''
                fts-test bash -c '
                  "$FTS_REAPER_EXECUTABLE" -newinst -nosplash -ignoreerrors &
                  RPID=$!
                  echo "Waiting for REAPER socket..."
                  SOCK=""
                  for i in $(seq 1 30); do
                    SOCK=$(ls /tmp/fts-daw-*.sock 2>/dev/null | head -1)
                    if [ -n "$SOCK" ]; then break; fi
                    sleep 1
                  done
                  if [ -z "$SOCK" ]; then
                    echo "No socket found after 30s"
                    kill $RPID 2>/dev/null
                    exit 1
                  fi
                  echo "Socket ready: $SOCK"
                  cargo test -p reaper-extension -- --ignored --nocapture
                  STATUS=$?
                  kill $RPID 2>/dev/null
                  exit $STATUS
                '
              '';
              fts-reaper-test.description = "Run REAPER integration tests (headless)";
            };

            claude.code = {
              enable = true;
              commands = {
                build = ''
                  Build the entire workspace

                  ```bash
                  fts-build
                  ```
                '';
                check = ''
                  Run clippy with warnings-as-errors

                  ```bash
                  fts-check
                  ```
                '';
                test = ''
                  Run all unit tests

                  ```bash
                  fts-test
                  ```
                '';
              };
            };

            git-hooks.hooks = {
              rustfmt.enable = true;
              clippy.enable = true;
            };

            enterShell = ''
              [ -f .env ] && { set -a; source .env; set +a; }
              echo ""
              echo "  FastTrackStudio dev shell (devenv)"
              echo "  ────────────────────────────────────────"
              echo "  fts-build        — cargo build --workspace"
              echo "  fts-check        — clippy (warnings-as-errors)"
              echo "  fts-fmt          — check formatting"
              echo "  fts-test         — cargo nextest run --workspace"
            '' + lib.optionalString pkgs.stdenv.isLinux ''
              echo "  fts-smoke        — REAPER headless smoke test"
              echo "  fts-reaper-test  — REAPER integration tests"
              echo "  fts-gui          — launch REAPER with GUI"
            '' + ''
              echo ""
              echo "  Rust: $(rustc --version)"
              echo "  dx:   $(dx --version 2>/dev/null || echo 'not available')"
              echo ""
            '';
          };

          devenv.shells.ci = let
            ftsCi = lib.optionalAttrs pkgs.stdenv.isLinux (
              fts-flake.lib.mkFtsPackages {
                inherit pkgs;
                cfg = fts-flake.presets.ci;
              }
            );
          in {
            cachix.pull = [ "fasttrackstudio" ];

            packages = with pkgs; [
              rustToolchain
              cargo-nextest
            ]
            ++ buildInputs
            ++ nativeBuildInputs
            ++ lib.optionals pkgs.stdenv.isLinux [
              ftsCi.fts-test
              ftsCi.reaper-fhs
            ];

            env = {
              LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
              OPENSSL_DIR = "${pkgs.openssl.dev}";
              OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
              CC_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.clang}/bin/clang";
              AR_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.bintools}/bin/llvm-ar";
            }
            // lib.optionalAttrs pkgs.stdenv.isLinux {
              LD_LIBRARY_PATH = libPath;
              FTS_REAPER_EXECUTABLE = "${ftsCi.reaper}/bin/reaper";
              FTS_REAPER_RESOURCES = "${ftsCi.reaper}/opt/REAPER";
            };
          };
        };
    };
}

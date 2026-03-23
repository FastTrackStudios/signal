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
    # devenv needs impure evaluation (builtins.getEnv for project root).
    # This allows `nix develop` to work without --impure.
    pure-eval = false;
  };

  outputs = { self, flake-parts, crane, devenv, devenv-root, nix2container, fts-flake, ... } @inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [ inputs.devenv.flakeModule ];

      systems = [ "x86_64-linux" "x86_64-darwin" "aarch64-darwin" "aarch64-linux" ];

      perSystem = { self', config, pkgs, lib, system, ... }:
        let
          # devenv needs to know the project root for impure operations.
          # When using direnv (.envrc), the devenv-root input is overridden to point
          # to .devenv/devenv.root. For direct `nix develop --impure`, we fall back
          # to $PWD (requires --impure).
          devenvRootFromInput = let
            content = builtins.readFile devenv-root.outPath;
          in pkgs.lib.strings.trim content;
          devenvRoot =
            if devenvRootFromInput != ""
            then devenvRootFromInput
            else builtins.getEnv "PWD";

          # Rust toolchain with WASM support
          # Pin to 1.94.0 — keep devenv git-hooks in sync via packageOverrides.
          rustToolchain = pkgs.rust-bin.stable."1.94.0".default.override {
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
            avahi avahi.dev
            glib gtk3 libsoup_3 webkitgtk_4_1 xdotool
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
              libx11 libxcb libxkbcommon wayland
              webkitgtk_4_1 libsoup_3
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

            # FastTrackStudio Web App (WASM)
            fasttrackstudio-web = craneLib.buildPackage (commonArgs // {
              pname = "fasttrackstudio-web";
              version = rev;
              inherit cargoArtifacts;
              buildPhaseCargoCommand = ''
                cd apps/web
                dx build --release --platform web
              '';
              installPhaseCommand = ''
                mkdir -p $out/www
                cp -r apps/web/target/dx/fasttrackstudio-web/release/web/* $out/www/
              '';
              doCheck = false;
            });

            # FastTrackStudio Desktop App
            fasttrackstudio-desktop = craneLib.buildPackage (commonArgs // {
              pname = "fasttrackstudio-desktop";
              version = rev;
              inherit cargoArtifacts;
              buildPhaseCargoCommand = ''
                cd apps/desktop
                dx build --release --platform desktop
              '';
              installPhaseCommand = ''
                mkdir -p $out/Applications $out/bin
                if [ -d "apps/desktop/target/dx/fasttrackstudio-desktop/release/macos" ]; then
                  cp -r apps/desktop/target/dx/fasttrackstudio-desktop/release/macos/*.app $out/Applications/
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
              ftsWrapper = fts-flake.wrapperPackages.${system} or {};
              script = pkgs.writeShellScript "create-installer-dmg" ''
                set -euo pipefail

                INSTALLER_APP="${self'.packages.fts-installer}/Applications"
                FTS_CONTROL_APP="${self'.packages.fasttrackstudio-desktop}/Applications"
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
          # Apps — full distribution DMG (REAPER + extensions + FTS apps)
          # ============================================================
          apps.create-distribution-dmg = let
            ftsWrapper = fts-flake.wrapperPackages.${system} or {};
          in {
            type = "app";
            program = let
              script = pkgs.writeShellScript "create-distribution-dmg" (''
                set -euo pipefail

                OUTPUT="FastTrackStudio-${rev}.dmg"
                STAGING=$(mktemp -d)
                FTS_DIR="$STAGING/FastTrackStudio"
                mkdir -p "$FTS_DIR"

                echo "Assembling FastTrackStudio distribution DMG..."
              '' + lib.optionalString pkgs.stdenv.isDarwin ''
                # ── FastTrack REAPER (REAPER + SWS + ReaPack) ──
                ${lib.optionalString (ftsWrapper ? reaper) ''
                  echo "  Bundling FastTrack REAPER..."
                  REAPER_APP="$FTS_DIR/FastTrack REAPER.app"
                  mkdir -p "$REAPER_APP/Contents/MacOS"
                  mkdir -p "$REAPER_APP/Contents/Resources/FTS/UserPlugins"
                  mkdir -p "$REAPER_APP/Contents/Resources/FTS/Scripts"

                  cp -r ${ftsWrapper.reaper}/Applications/REAPER.app "$REAPER_APP/Contents/Resources/REAPER.app"
                  cp ${ftsWrapper.sws}/UserPlugins/*.dylib "$REAPER_APP/Contents/Resources/FTS/UserPlugins/"
                  cp ${ftsWrapper.reapack}/UserPlugins/*.dylib "$REAPER_APP/Contents/Resources/FTS/UserPlugins/"
                  cp ${ftsWrapper.sws}/Scripts/*.py "$REAPER_APP/Contents/Resources/FTS/Scripts/" 2>/dev/null || true

                  # FTS icon
                  ${lib.optionalString (ftsWrapper ? icon) ''
                    cp ${ftsWrapper.icon}/fts-reaper.icns "$REAPER_APP/Contents/Resources/fts-reaper.icns"
                  ''}

                  # Launcher script
                  cat > "$REAPER_APP/Contents/MacOS/FastTrack REAPER" << 'LAUNCHER'
                #!/bin/bash
                set -euo pipefail
                APP_DIR="$(cd "$(dirname "$0")/.." && pwd)"
                REAPER_BIN="$APP_DIR/Resources/REAPER.app/Contents/MacOS/REAPER"
                FTS_EXTENSIONS="$APP_DIR/Resources/FTS/UserPlugins"
                FTS_SCRIPTS="$APP_DIR/Resources/FTS/Scripts"
                CONFIG_DIR="$HOME/Library/Application Support/FastTrackStudio/Reaper"
                mkdir -p "$CONFIG_DIR/UserPlugins" "$CONFIG_DIR/Scripts"
                for dylib in "$FTS_EXTENSIONS"/*.dylib; do
                  [ -f "$dylib" ] && ln -sf "$dylib" "$CONFIG_DIR/UserPlugins/"
                done
                for script in "$FTS_SCRIPTS"/*.py; do
                  [ -f "$script" ] && ln -sf "$script" "$CONFIG_DIR/Scripts/"
                done
                exec "$REAPER_BIN" -cfgfile "$CONFIG_DIR/reaper.ini" "$@"
                LAUNCHER
                  chmod +x "$REAPER_APP/Contents/MacOS/FastTrack REAPER"

                  cat > "$REAPER_APP/Contents/Info.plist" << PLIST
                <?xml version="1.0" encoding="UTF-8"?>
                <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
                <plist version="1.0">
                <dict>
                  <key>CFBundleName</key>
                  <string>FastTrack REAPER</string>
                  <key>CFBundleDisplayName</key>
                  <string>FastTrack REAPER</string>
                  <key>CFBundleIdentifier</key>
                  <string>com.fasttrackstudio.reaper</string>
                  <key>CFBundleVersion</key>
                  <string>${ftsWrapper.reaper.version}</string>
                  <key>CFBundleShortVersionString</key>
                  <string>${ftsWrapper.reaper.version}</string>
                  <key>CFBundleExecutable</key>
                  <string>FastTrack REAPER</string>
                  <key>CFBundleIconFile</key>
                  <string>fts-reaper.icns</string>
                  <key>CFBundlePackageType</key>
                  <string>APPL</string>
                  <key>NSHighResolutionCapable</key>
                  <true/>
                </dict>
                </plist>
                PLIST
                ''}

                # ── FTS Control ──
                FTS_CONTROL_APP="${self'.packages.fasttrackstudio-desktop}/Applications"
                if [ -d "$FTS_CONTROL_APP" ]; then
                  echo "  Bundling FTS Control..."
                  cp -r "$FTS_CONTROL_APP"/*.app "$FTS_DIR/"
                fi

                # ── FTS Installer ──
                INSTALLER_APP="${self'.packages.fts-installer}/Applications"
                if [ -d "$INSTALLER_APP" ]; then
                  echo "  Bundling FTS Installer..."
                  cp -r "$INSTALLER_APP"/*.app "$FTS_DIR/"
                fi

                # ── Applications symlink for drag-to-install ──
                ln -s /Applications "$STAGING/Applications"

                # ── Create the DMG ──
                hdiutil create -volname "FastTrackStudio ${rev}" \
                  -srcfolder "$STAGING" \
                  -ov -format UDZO \
                  "$OUTPUT"

                rm -rf "$STAGING"
                echo "Created $OUTPUT ($(du -h "$OUTPUT" | cut -f1))"
              '' + lib.optionalString pkgs.stdenv.isLinux ''
                echo "Linux distribution packaging not yet implemented."
                echo "Use 'nix build .#fts-test' or 'nix build .#fts-gui' for Linux."
                exit 1
              '');
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
            ftsWrapper = fts-flake.wrapperPackages.${system} or {};
          in {
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
            // lib.optionalAttrs pkgs.stdenv.isDarwin ({
              DYLD_LIBRARY_PATH = libPath;
            } // lib.optionalAttrs (ftsWrapper ? reaper) {
              FTS_REAPER_EXECUTABLE = "${ftsWrapper.reaper}/bin/reaper";
            });

            scripts = {
              fts-check.exec = "cargo clippy --workspace -- -D warnings";
              fts-check.description = "Run clippy with warnings-as-errors";

              fts-fmt.exec = "cargo fmt --all -- --check";
              fts-fmt.description = "Check formatting";

              fts-unit-test.exec = "cargo nextest run --workspace";
              fts-unit-test.description = "Run all unit tests";

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

              fts-reaper-test.exec = "cargo xtask reaper-test \"$@\"";
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
                  fts-unit-test
                  ```
                '';
              };
            };

            # NOTE: git-hooks are managed by beads (core.hooksPath = .beads/hooks).
            # Enabling devenv git-hooks here causes "Cowardly refusing to install
            # hooks with core.hooksPath set" and blocks shell entry entirely.
            # Formatting is enforced by the beads pre-commit hook instead.

            enterShell = ''
              [ -f .env ] && { set -a; source .env; set +a; }
              echo ""
              echo "  FastTrackStudio dev shell (devenv)"
              echo "  ────────────────────────────────────────"
              echo "  fts-build        — cargo build --workspace"
              echo "  fts-check        — clippy (warnings-as-errors)"
              echo "  fts-fmt          — check formatting"
              echo "  fts-unit-test    — cargo nextest run --workspace"
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
            devenv.root =
              pkgs.lib.mkIf (devenvRoot != "") devenvRoot;

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

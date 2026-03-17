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
  };

  outputs = { self, flake-parts, crane, ... } @inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
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
          # Dev Shell
          # ============================================================
          devShells.default = pkgs.mkShell {
            name = "fasttrackstudio-dev";
            inherit buildInputs nativeBuildInputs;

            packages = with pkgs; [
              rustToolchain
              dioxus-cli
              wasm-bindgen-cli
              tailwindcss_4
              cargo-watch
              cargo-nextest
              bacon
              nodejs_22  # For Playwright tests
            ]
            ++ lib.optionals pkgs.stdenv.isLinux (with pkgs; [
              xvfb-run  # Virtual framebuffer for headless REAPER tests
              reaper    # REAPER DAW for integration tests
            ]);

            shellHook = ''
              ${if pkgs.stdenv.isLinux then ''
                export LD_LIBRARY_PATH="${libPath}:$LD_LIBRARY_PATH"
                export XDG_DATA_DIRS="${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}:$XDG_DATA_DIRS"
              '' else ''
                export DYLD_LIBRARY_PATH="${libPath}:$DYLD_LIBRARY_PATH"
              ''}
              export CC_wasm32_unknown_unknown="${pkgs.llvmPackages_18.clang}/bin/clang"
              export AR_wasm32_unknown_unknown="${pkgs.llvmPackages_18.bintools}/bin/llvm-ar"
              export RUST_SRC_PATH="${rustToolchain}/lib/rustlib/src/rust/library"
              [ -f .env ] && { set -a; source .env; set +a; }
              echo "FastTrackStudio dev environment (${system})"
              echo "  - Rust: $(rustc --version)"
              echo "  - dx: $(dx --version)"
              echo "  - wasm-bindgen: $(wasm-bindgen --version)"
            '';

            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
            OPENSSL_DIR = "${pkgs.openssl.dev}";
            OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
          };
        };
    };
}

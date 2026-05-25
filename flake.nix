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
    nix2container.url = "github:nlewo/nix2container";
    nix2container.inputs.nixpkgs.follows = "nixpkgs";
    # Shared Dioxus dev environment + build conventions. This repo is the
    # user-facing Dioxus app, so the dev shell layers on top of the
    # FastTrackStudio Dioxus flake's shell (toolchain, dx, wasm-bindgen),
    # the same way fts-ui / session do. See dioxus-flake README.
    dioxus-flake.url = "github:FastTrackStudios/Dioxus-Flake";
    dioxus-flake.inputs.nixpkgs.follows = "nixpkgs";
    dioxus-flake.inputs.rust-overlay.follows = "rust-overlay";
    dioxus-flake.inputs.crane.follows = "crane";
    fts-flake.url = "github:FastTrackStudios/fts-flake";
    session.url = "github:FastTrackStudios/session";
    session.inputs.nixpkgs.follows = "nixpkgs";
    sync.url = "github:FastTrackStudios/sync";
    sync.inputs.nixpkgs.follows = "nixpkgs";
    signal.url = "github:FastTrackStudios/signal";
    signal.inputs.nixpkgs.follows = "nixpkgs";
    daw.url = "github:FastTrackStudios/daw";
    daw.inputs.nixpkgs.follows = "nixpkgs";
    # WDL library — submodule of reaper-rs, needed for reaper-low C++ build
    wdl = {
      url = "github:justinfrankel/WDL";
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

  outputs = { self, flake-parts, crane, nix2container, dioxus-flake, fts-flake, session, sync, signal, daw, wdl, ... } @inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" "x86_64-darwin" "aarch64-darwin" "aarch64-linux" ];

      perSystem = { self', config, pkgs, lib, system, ... }:
        let
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
              (lib.fileset.fileFilter (f: f.hasExt "png") ./.)
              (lib.fileset.fileFilter (f: f.hasExt "ttf") ./.)
              (lib.fileset.fileFilter (f: f.hasExt "svg") ./.)
              (lib.fileset.fileFilter (f: f.name == "Dioxus.toml") ./.)
              (lib.fileset.fileFilter (f: f.name == "tailwind-config.js") ./.)
            ];
          };

          pkgConfigInputs = with pkgs; [
            openssl.dev
            fontconfig.dev
            freetype.dev
          ];

          pkgConfigPath = lib.concatStringsSep ":" [
            (lib.makeSearchPathOutput "dev" "lib/pkgconfig" pkgConfigInputs)
            (lib.makeSearchPathOutput "dev" "share/pkgconfig" pkgConfigInputs)
          ];

          # Build dependencies
          buildInputs = (with pkgs; [
            openssl
            libiconv
            fontconfig
            freetype
            cmake
            python3
          ])
          ++ pkgConfigInputs
          ++ lib.optionals pkgs.stdenv.isLinux (with pkgs; [
            alsa-lib alsa-lib.dev
            avahi avahi.dev
            glib gtk3 gdk-pixbuf pango cairo atk
            libsoup_3 webkitgtk_4_1 xdotool
            libx11 libxcursor libxrandr libxi libxcb
            libxkbcommon wayland libGL vulkan-loader
          ])
          ++ lib.optionals pkgs.stdenv.isDarwin (with pkgs; [
            apple-sdk_15
            libiconv
          ]);

          # Stub codesign for Nix sandbox builds — dx always tries to
          # invoke codesign on macOS even with --codesign false.
          fakeCodesign = pkgs.writeShellScriptBin "codesign" ''exec true'';

          nativeBuildInputs = with pkgs; [
            pkg-config
            rustPlatform.bindgenHook
            dioxus-cli
            wasm-bindgen-cli
            tailwindcss_4
          ] ++ lib.optionals pkgs.stdenv.isDarwin [
            fakeCodesign
          ];

          # Vendor cargo deps with WDL submodule injected into reaper-rs
          cargoVendorDir = craneLib.vendorCargoDeps {
            inherit src;
            overrideVendorGitCheckout = ps: drv:
              if lib.any (p: lib.hasInfix "reaper-rs" (p.source or "")) ps
              then drv.overrideAttrs (old: {
                postInstall = (old.postInstall or "") + ''
                  for d in $out/reaper-low-*/; do
                    mkdir -p "$d/lib/WDL"
                    cp -r ${wdl}/* "$d/lib/WDL/"
                  done
                '';
              })
              else drv;
          };

          # Common args for all crane builds
          commonArgs = {
            inherit src buildInputs nativeBuildInputs cargoVendorDir;
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
              gdk-pixbuf pango cairo atk
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

            # FastTrackStudio Web App (WASM)
            fasttrackstudio-web = craneLib.buildPackage (commonArgs // {
              pname = "fasttrackstudio-web";
              version = rev;
              inherit cargoArtifacts;
              doNotPostBuildInstallCargoBinaries = true;
              buildPhaseCargoCommand = ''
                cd apps/web
                dx build --release --platform web
              '';
              installPhaseCommand = ''
                cd "$NIX_BUILD_TOP/source"
                mkdir -p $out/www
                cp -r target/dx/fasttrackstudio-web/release/web/* $out/www/
              '';
              doCheck = false;
            });

            # FastTrackStudio Desktop App
            fasttrackstudio-desktop = craneLib.buildPackage (commonArgs // {
              pname = "fasttrackstudio-desktop";
              version = rev;
              inherit cargoArtifacts;
              doNotPostBuildInstallCargoBinaries = true;
              buildPhaseCargoCommand = ''
                cd apps/desktop
                dx build --release --platform desktop
              '';
              installPhaseCommand = ''
                cd "$NIX_BUILD_TOP/source"
                mkdir -p $out/Applications $out/bin
                if [ -d "target/dx/fasttrackstudio-desktop/release/macos" ]; then
                  cp -r target/dx/fasttrackstudio-desktop/release/macos/*.app $out/Applications/FastTrackStudio.app
                  ln -s "$out/Applications/FastTrackStudio.app/Contents/MacOS/fasttrackstudio-desktop" $out/bin/fasttrackstudio-desktop
                elif [ -f "target/release/fasttrackstudio-desktop" ]; then
                  cp target/release/fasttrackstudio-desktop $out/bin/
                fi
              '';
              doCheck = false;
            });

            # FTS Installer Desktop App
            fts-installer = craneLib.buildPackage (commonArgs // {
              pname = "fts-installer";
              version = rev;
              inherit cargoArtifacts;
              doNotPostBuildInstallCargoBinaries = true;
              buildPhaseCargoCommand = ''
                cd apps/installer
                dx build --release --platform desktop
              '';
              installPhaseCommand = ''
                cd "$NIX_BUILD_TOP/source"
                mkdir -p $out/Applications $out/bin
                if [ -d "target/dx/fts-installer/release/macos" ]; then
                  cp -r target/dx/fts-installer/release/macos/*.app $out/Applications/
                  ln -s "$out/Applications/"*.app"/Contents/MacOS/"* $out/bin/fts-installer
                elif [ -f "target/release/fts-installer" ]; then
                  cp target/release/fts-installer $out/bin/
                fi
              '';
              doCheck = false;
            });

            default = self'.packages.fasttrackstudio-desktop;
          } // lib.optionalAttrs pkgs.stdenv.isLinux {
            # ── Portable Linux bundle ──────────────────────────────────
            # Self-contained directory with wrapper scripts that invoke
            # the bundled ld-linux, so it runs on any x86_64 Linux
            # without Nix installed.
            linux-bundle = pkgs.stdenv.mkDerivation {
              pname = "FastTrackStudio";
              version = rev;
              dontUnpack = true;
              nativeBuildInputs = [ pkgs.patchelf ];

              buildPhase = ''
                mkdir -p pkg/{bin/.unwrapped,lib}

                # ── Collect binaries ──
                for p in ${self'.packages.fts-installer} ${self'.packages.fasttrackstudio-desktop}; do
                  if [ -d "$p/bin" ]; then
                    for f in "$p"/bin/*; do
                      [ -f "$f" ] && cp "$f" pkg/bin/.unwrapped/
                    done
                  fi
                done

                # ── Collect Nix-store shared libraries ──
                collect_libs() {
                  ldd "$1" 2>/dev/null | while IFS= read -r line; do
                    lib_path=$(echo "$line" | sed -n 's|.*=> \(/nix/store/[^ ]*\).*|\1|p')
                    [ -z "$lib_path" ] || [ ! -f "$lib_path" ] && continue
                    name=$(basename "$lib_path")
                    [ -f "pkg/lib/$name" ] && continue
                    cp -L "$lib_path" "pkg/lib/$name"
                  done
                }

                for bin in pkg/bin/.unwrapped/*; do collect_libs "$bin"; done
                # Transitive deps — two extra passes
                for lib in pkg/lib/*; do collect_libs "$lib"; done
                for lib in pkg/lib/*; do collect_libs "$lib"; done

                # ── Bundle the dynamic linker ──
                INTERP=$(patchelf --print-interpreter pkg/bin/.unwrapped/* 2>/dev/null | head -1 || true)
                if [ -n "$INTERP" ] && [ -f "$INTERP" ]; then
                  INTERP_NAME=$(basename "$INTERP")
                  cp -L "$INTERP" "pkg/lib/$INTERP_NAME"
                else
                  echo "WARNING: could not determine ELF interpreter" >&2
                  INTERP_NAME="ld-linux-x86-64.so.2"
                fi

                # ── Create wrapper scripts ──
                for bin in pkg/bin/.unwrapped/*; do
                  name=$(basename "$bin")
                  cat > "pkg/bin/$name" << WRAPPER
#!/bin/bash
DIR="\$(cd "\$(dirname "\$0")" && pwd)"
exec "\$DIR/../lib/$INTERP_NAME" --library-path "\$DIR/../lib" "\$DIR/.unwrapped/$name" "\$@"
WRAPPER
                  chmod +x "pkg/bin/$name"
                done
              '';

              installPhase = ''
                mkdir -p $out
                cp -r pkg/* $out/
                cp ${./scripts/linux-install.sh} $out/install.sh
                chmod +x $out/install.sh
              '';
            };
          };

          # ============================================================
          # Apps — installer DMG
          # ============================================================
          apps.create-installer-dmg = let
            # TODO: re-enable once session/signal/sync flakes build on macOS
            sessionExt = null;
            syncExt = null;
            signalExt = null;
            signalClap = null;
            dawBridge = daw.packages.${system}.daw-bridge or null;
            reaper-launcher = daw.packages.${system}.reaper-launcher or null;
          in {
            type = "app";
            program = let
              script = pkgs.writeShellScript "create-installer-dmg" ''
                set -euo pipefail

                INSTALLER_APP="${self'.packages.fts-installer}/Applications"
                OUTPUT="FastTrackStudio-Installer.dmg"
                STAGING=$(mktemp -d)

                echo "Assembling installer DMG..."

                if [ -d "$INSTALLER_APP" ]; then
                  cp -r "$INSTALLER_APP"/*.app "$STAGING/"
                else
                  echo "ERROR: FTS Installer app not found at $INSTALLER_APP"
                  exit 1
                fi

                # Find the .app we just copied
                APP=$(ls -d "$STAGING"/*.app | head -1)
                RESOURCES="$APP/Contents/Resources"
                mkdir -p "$RESOURCES/fts-bundle/extensions" \
                         "$RESOURCES/fts-bundle/fx" \
                         "$RESOURCES/fts-bundle/daw-bridge"

                # Bundle reaper-launcher
                ${lib.optionalString (reaper-launcher != null) ''
                  cp ${reaper-launcher}/bin/reaper-launcher "$RESOURCES/fts-bundle/"
                  echo "  Bundled reaper-launcher"
                ''}

                # Bundle FTS extensions
                ${lib.optionalString (sessionExt != null) ''
                  cp ${sessionExt}/bin/session-extension "$RESOURCES/fts-bundle/extensions/"
                  echo "  Bundled session-extension"
                ''}
                ${lib.optionalString (syncExt != null) ''
                  cp ${syncExt}/bin/sync-extension "$RESOURCES/fts-bundle/extensions/"
                  echo "  Bundled sync-extension"
                ''}
                ${lib.optionalString (signalExt != null) ''
                  cp ${signalExt}/bin/signal-extension "$RESOURCES/fts-bundle/extensions/"
                  echo "  Bundled signal-extension"
                ''}

                # Bundle CLAP plugins
                ${lib.optionalString (signalClap != null) ''
                  cp -r ${signalClap}/lib/clap/"FTS Signal Controller.clap" "$RESOURCES/fts-bundle/fx/"
                  echo "  Bundled FTS Signal Controller CLAP"
                ''}

                # Bundle daw-bridge
                ${lib.optionalString (dawBridge != null) ''
                  cp ${dawBridge}/lib/libreaper_daw_bridge.dylib "$RESOURCES/fts-bundle/daw-bridge/"
                  echo "  Bundled daw-bridge"
                ''}

                # Create the DMG
                hdiutil create -volname "FastTrackStudio Installer" \
                  -srcfolder "$STAGING" \
                  -ov -format UDZO \
                  "$OUTPUT"

                chmod -R u+w "$STAGING" 2>/dev/null || true
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
            sessionExt = session.packages.${system}.session-extension or null;
            syncExt = sync.packages.${system}.sync-extension or null;
            signalExt = signal.packages.${system}.signal-extension or null;
            signalClap = signal.packages.${system}.fts-signal-controller or null;
            dawBridge = daw.packages.${system}.daw-bridge or null;
          in {
            type = "app";
            program = let
              script = pkgs.writeShellScript "create-distribution-dmg" (''
                set -euo pipefail

                OUTPUT="FastTrackStudio.dmg"
                STAGING=$(mktemp -d)
                FTS_DIR="$STAGING/FastTrackStudio"
                mkdir -p "$FTS_DIR"

                echo "Assembling FastTrackStudio distribution DMG..."
              '' + lib.optionalString pkgs.stdenv.isDarwin ''
                # ── FTS-Reaper (REAPER + SWS + ReaPack + extensions) ──
                ${lib.optionalString (ftsWrapper ? reaper) ''
                  echo "  Bundling FTS-Reaper..."
                  REAPER_APP="$FTS_DIR/FTS-Reaper.app"
                  mkdir -p "$REAPER_APP/Contents/MacOS"
                  mkdir -p "$REAPER_APP/Contents/Resources/FTS/UserPlugins/fts-extensions"
                  mkdir -p "$REAPER_APP/Contents/Resources/FTS/UserPlugins/FX/FTS"
                  mkdir -p "$REAPER_APP/Contents/Resources/FTS/UserPlugins/daw-bridge"
                  mkdir -p "$REAPER_APP/Contents/Resources/FTS/Scripts"

                  cp -r ${ftsWrapper.reaper}/Applications/REAPER.app "$REAPER_APP/Contents/Resources/REAPER.app"
                  cp ${ftsWrapper.sws}/UserPlugins/*.dylib "$REAPER_APP/Contents/Resources/FTS/UserPlugins/"
                  cp ${ftsWrapper.reapack}/UserPlugins/*.dylib "$REAPER_APP/Contents/Resources/FTS/UserPlugins/"
                  cp ${ftsWrapper.sws}/Scripts/*.py "$REAPER_APP/Contents/Resources/FTS/Scripts/" 2>/dev/null || true

                  # SHM guest extensions (fts-extensions/)
                  ${lib.optionalString (sessionExt != null) ''
                    cp ${sessionExt}/bin/session-extension "$REAPER_APP/Contents/Resources/FTS/UserPlugins/fts-extensions/"
                    echo "  Bundled session-extension"
                  ''}

                  ${lib.optionalString (syncExt != null) ''
                    cp ${syncExt}/bin/sync-extension "$REAPER_APP/Contents/Resources/FTS/UserPlugins/fts-extensions/"
                    echo "  Bundled sync-extension"
                  ''}

                  ${lib.optionalString (signalExt != null) ''
                    cp ${signalExt}/bin/signal-extension "$REAPER_APP/Contents/Resources/FTS/UserPlugins/fts-extensions/"
                    echo "  Bundled signal-extension"
                  ''}

                  # CLAP plugins (UserPlugins/FX/FTS/)
                  ${lib.optionalString (signalClap != null) ''
                    cp -r ${signalClap}/lib/clap/"FTS Signal Controller.clap" "$REAPER_APP/Contents/Resources/FTS/UserPlugins/FX/FTS/"
                    echo "  Bundled FTS Signal Controller CLAP plugin"
                  ''}

                  # DAW bridge (REAPER extension plugin)
                  ${lib.optionalString (dawBridge != null) ''
                    cp ${dawBridge}/lib/libreaper_daw_bridge.dylib "$REAPER_APP/Contents/Resources/FTS/UserPlugins/daw-bridge/"
                    echo "  Bundled daw-bridge"
                  ''}

                  # FTS icon
                  ${lib.optionalString (ftsWrapper ? icon) ''
                    cp ${ftsWrapper.icon}/fts-reaper.icns "$REAPER_APP/Contents/Resources/fts-reaper.icns"
                  ''}

                  # Launcher script
                  cat > "$REAPER_APP/Contents/MacOS/FTS-Reaper" << 'LAUNCHER'
                #!/bin/bash
                set -euo pipefail
                APP_DIR="$(cd "$(dirname "$0")/.." && pwd)"
                REAPER_BIN="$APP_DIR/Resources/REAPER.app/Contents/MacOS/REAPER"
                FTS_EXTENSIONS="$APP_DIR/Resources/FTS/UserPlugins"
                FTS_SCRIPTS="$APP_DIR/Resources/FTS/Scripts"
                CONFIG_DIR="$HOME/Library/Application Support/FastTrackStudio/Reaper"
                mkdir -p "$CONFIG_DIR/UserPlugins/fts-extensions" "$CONFIG_DIR/UserPlugins/FX/FTS" "$CONFIG_DIR/UserPlugins/daw-bridge" "$CONFIG_DIR/Scripts"
                for dylib in "$FTS_EXTENSIONS"/*.dylib; do
                  [ -f "$dylib" ] && ln -sf "$dylib" "$CONFIG_DIR/UserPlugins/"
                done
                for script in "$FTS_SCRIPTS"/*.py; do
                  [ -f "$script" ] && ln -sf "$script" "$CONFIG_DIR/Scripts/"
                done
                # Link SHM guest extensions (sync, signal, session, etc.)
                for ext in "$FTS_EXTENSIONS/fts-extensions"/*; do
                  [ -f "$ext" ] && ln -sf "$ext" "$CONFIG_DIR/UserPlugins/fts-extensions/"
                done
                # Link CLAP plugins (FX/FTS/)
                for clap in "$FTS_EXTENSIONS/FX/FTS"/*; do
                  [ -e "$clap" ] && ln -sf "$clap" "$CONFIG_DIR/UserPlugins/FX/FTS/"
                done
                # Link daw-bridge
                for lib in "$FTS_EXTENSIONS/daw-bridge"/*; do
                  [ -f "$lib" ] && ln -sf "$lib" "$CONFIG_DIR/UserPlugins/daw-bridge/"
                done
                exec "$REAPER_BIN" -cfgfile "$CONFIG_DIR/reaper.ini" "$@"
                LAUNCHER
                  chmod +x "$REAPER_APP/Contents/MacOS/FTS-Reaper"

                  cat > "$REAPER_APP/Contents/Info.plist" << PLIST
                <?xml version="1.0" encoding="UTF-8"?>
                <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
                <plist version="1.0">
                <dict>
                  <key>CFBundleName</key>
                  <string>FTS-Reaper</string>
                  <key>CFBundleDisplayName</key>
                  <string>FTS-Reaper</string>
                  <key>CFBundleIdentifier</key>
                  <string>com.fasttrackstudio.reaper</string>
                  <key>CFBundleVersion</key>
                  <string>${ftsWrapper.reaper.version}</string>
                  <key>CFBundleShortVersionString</key>
                  <string>${ftsWrapper.reaper.version}</string>
                  <key>CFBundleExecutable</key>
                  <string>FTS-Reaper</string>
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

                # ── FastTrackStudio Desktop (with embedded web app) ──
                FTS_DESKTOP_APP="${self'.packages.fasttrackstudio-desktop}/Applications"
                FTS_WEB_APP="${self'.packages.fasttrackstudio-web}/www"
                if [ -d "$FTS_DESKTOP_APP" ]; then
                  echo "  Bundling FastTrackStudio Desktop..."
                  cp -r "$FTS_DESKTOP_APP"/*.app "$FTS_DIR/"

                  # Find the .app we just copied
                  FTS_APP=$(ls -d "$FTS_DIR"/*.app 2>/dev/null | grep -i fasttrack | grep -iv FTS-Reaper | grep -iv Installer | head -1)
                  if [ -n "$FTS_APP" ] && [ -d "$FTS_WEB_APP" ]; then
                    echo "  Embedding web app into desktop bundle..."
                    mkdir -p "$FTS_APP/Contents/Resources/www"
                    cp -r "$FTS_WEB_APP"/* "$FTS_APP/Contents/Resources/www/"

                    # Wrap the original executable to set GATEWAY_WS_STATIC_DIR
                    ORIGINAL_BIN=$(defaults read "$FTS_APP/Contents/Info.plist" CFBundleExecutable 2>/dev/null || basename "$FTS_APP" .app)
                    if [ -f "$FTS_APP/Contents/MacOS/$ORIGINAL_BIN" ]; then
                      mv "$FTS_APP/Contents/MacOS/$ORIGINAL_BIN" "$FTS_APP/Contents/MacOS/.''${ORIGINAL_BIN}-wrapped"
                      cat > "$FTS_APP/Contents/MacOS/$ORIGINAL_BIN" << 'WRAPPER'
#!/bin/bash
APP_DIR="$(cd "$(dirname "$0")/.." && pwd)"
export GATEWAY_WS_STATIC_DIR="$APP_DIR/Resources/www"
exec "$APP_DIR/MacOS/.$(basename "$0")-wrapped" "$@"
WRAPPER
                      chmod +x "$FTS_APP/Contents/MacOS/$ORIGINAL_BIN"
                    fi
                  fi
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

                chmod -R u+w "$STAGING" 2>/dev/null || true
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
          # Dev Shells — layered on the dioxus-flake shell (no devenv)
          # ============================================================
          # `inputsFrom` pulls the dioxus-flake shell's build inputs (rust
          # toolchain, dx, wasm-bindgen, gtk/webkit) — but NOT its env vars or
          # shellHook, so we re-declare those below (see fts-ui / session).
          devShells = let
            ftsPkgs = lib.optionalAttrs pkgs.stdenv.isLinux (
              fts-flake.lib.mkFtsPackages {
                inherit pkgs;
                cfg = fts-flake.presets.dev;
              }
            );
            ftsCi = lib.optionalAttrs pkgs.stdenv.isLinux (
              fts-flake.lib.mkFtsPackages {
                inherit pkgs;
                cfg = fts-flake.presets.ci;
              }
            );
            ftsWrapper = fts-flake.wrapperPackages.${system} or {};

            commonEnv = {
              LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
              OPENSSL_DIR = "${pkgs.openssl.dev}";
              OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
              PKG_CONFIG_PATH = pkgConfigPath;
              CC_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.clang}/bin/clang";
              AR_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.bintools}/bin/llvm-ar";
              RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
            };
          in {
            default = pkgs.mkShell ({
              inputsFrom = [ dioxus-flake.devShells.${system}.default ];

              packages = (with pkgs; [
                rustToolchain
                cargo-watch
                cargo-nextest
                bacon
              ])
              ++ buildInputs
              ++ nativeBuildInputs
              ++ lib.optionals pkgs.stdenv.isLinux [
                ftsPkgs.fts-test
                ftsPkgs.fts-gui
                ftsPkgs.reaper-fhs
              ];

              # dioxus-flake's own shellHook (inherited via inputsFrom) pins
              # dx and sets up LD_LIBRARY_PATH; we only add project aliases.
              shellHook = ''
                [ -f .env ] && { set -a; source .env; set +a; }
                export PATH="$HOME/.cargo/bin:$PATH"

                alias fts-build='cargo build --workspace'
                alias fts-check='cargo clippy --workspace -- -D warnings'
                alias fts-fmt='cargo fmt --all -- --check'
                alias fts-unit-test='cargo nextest run --workspace'
              '' + lib.optionalString pkgs.stdenv.isLinux ''
                alias fts-reaper-test='cargo xtask reaper-test'
              '' + ''
                echo ""
                echo "  FastTrackStudio dev shell (dioxus-flake)"
                echo "  ────────────────────────────────────────"
                echo "  fts-build        — cargo build --workspace"
                echo "  fts-check        — clippy (warnings-as-errors)"
                echo "  fts-fmt          — check formatting"
                echo "  fts-unit-test    — cargo nextest run --workspace"
              '' + lib.optionalString pkgs.stdenv.isLinux ''
                echo "  fts-reaper-test  — REAPER integration tests"
                echo "  fts-gui          — launch REAPER with GUI"
              '' + ''
                echo ""
                echo "  Rust: $(rustc --version)"
                echo "  dx:   $(dx --version 2>/dev/null || echo 'not available')"
                echo ""
              '';
            }
            // commonEnv
            // lib.optionalAttrs pkgs.stdenv.isLinux {
              LD_LIBRARY_PATH = libPath;
              XDG_DATA_DIRS = "${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}";
              # webkit2gtk GPU compositing fails to allocate GBM buffers on many
              # NixOS GPU/driver combos ("Failed to create GBM buffer"). Match
              # dioxus-flake's shell: force x11 + disable webkit GPU compositing.
              GDK_BACKEND = "x11";
              WEBKIT_DISABLE_COMPOSITING_MODE = "1";
              WEBKIT_DISABLE_DMABUF_RENDERER = "1";
              WEBKIT_ENABLE_WEBGPU = "0";
              GTK_USE_PORTAL = "0";
              # No AT-SPI accessibility bus on headless / minimal sessions;
              # silences webkit's "Can't connect to a11y bus" warning.
              NO_AT_BRIDGE = "1";
              FTS_REAPER_EXECUTABLE = "${ftsPkgs.reaper}/bin/reaper";
              FTS_REAPER_RESOURCES = "${ftsPkgs.reaper}/opt/REAPER";
            }
            // lib.optionalAttrs pkgs.stdenv.isDarwin ({
              DYLD_LIBRARY_PATH = libPath;
            } // lib.optionalAttrs (ftsWrapper ? reaper) {
              FTS_REAPER_EXECUTABLE = "${ftsWrapper.reaper}/bin/reaper";
            }));

            ci = pkgs.mkShell ({
              packages = (with pkgs; [
                rustToolchain
                cargo-nextest
              ])
              ++ buildInputs
              ++ nativeBuildInputs
              ++ lib.optionals pkgs.stdenv.isLinux [
                ftsCi.fts-test
                ftsCi.reaper-fhs
              ];
            }
            // commonEnv
            // lib.optionalAttrs pkgs.stdenv.isLinux {
              LD_LIBRARY_PATH = libPath;
              FTS_REAPER_EXECUTABLE = "${ftsCi.reaper}/bin/reaper";
              FTS_REAPER_RESOURCES = "${ftsCi.reaper}/opt/REAPER";
            });
          };
        };
    };
}

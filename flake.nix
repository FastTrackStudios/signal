{
  description = "Signal — plugin/signal-chain management for FastTrackStudio";

  inputs = {
    # Shared Dioxus toolchain hub — every FTS Dioxus repo follows its nixpkgs /
    # rust-overlay pins so `dx` and the Rust toolchain stay in lockstep across
    # signal / session / daw / fts-ui / the monorepo.
    dioxus-flake.url = "github:FastTrackStudios/Dioxus-Flake";
    nixpkgs.follows = "dioxus-flake/nixpkgs";
    flake-parts.url = "github:hercules-ci/flake-parts";
    rust-overlay.follows = "dioxus-flake/rust-overlay";
    crane.url = "github:ipetkov/crane";
    fts-flake.url = "github:FastTrackStudios/fts-flake";
  };

  nixConfig = {
    extra-trusted-public-keys = [
      "fasttrackstudio.cachix.org-1:r7v7WXBeSZ7m5meL6w0wttnvsOltRvTpXeVNItcy9f4="
    ];
    extra-substituters = [
      "https://fasttrackstudio.cachix.org"
    ];
  };

  outputs = { self, flake-parts, crane, fts-flake, dioxus-flake, ... } @inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" "x86_64-darwin" "aarch64-darwin" "aarch64-linux" ];

      perSystem = { self', config, pkgs, lib, system, ... }:
        let

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
            # JACK headers/lib for cpal's `jack` feature (signal-sampler).
            # At runtime, run under `pw-jack` to route through PipeWire.
            libjack2
            # libpipewire headers/lib for cpal's native `pipewire` feature —
            # talks to PipeWire directly (no JACK shim). `.dev` carries the
            # `libpipewire-0.3.pc` + headers that bindgen/pkg-config need.
            pipewire pipewire.dev
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
                dx build --release --platform desktop --no-default-features
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

            # Signal Extension — SHM guest process for REAPER integration
            signal-extension = craneLib.buildPackage (commonArgs // {
              pname = "signal-extension";
              version = rev;
              inherit cargoArtifacts;
              cargoExtraArgs = "-p signal-extension";
              doCheck = false;
            });

            # FTS Signal Controller — CLAP plugin bundle
            # Uses nih_plug_xtask bundler (via cargo xtask bundle) to produce
            # a proper .clap bundle: macOS = Contents/MacOS/ + Info.plist,
            # Linux/Windows = renamed .so/.dll.
            fts-signal-controller = craneLib.buildPackage (commonArgs // {
              pname = "fts-signal-controller";
              version = rev;
              inherit cargoArtifacts;
              buildPhaseCargoCommand = ''
                cargo xtask bundle fts-signal-controller --release
              '';
              installPhaseCommand = ''
                mkdir -p $out/lib/clap
                cp -r target/bundled/"FTS Signal Controller.clap" $out/lib/clap/
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
          # Dev Shell — plain `pkgs.mkShell` (no devenv), matching the
          # other FTS repos. Toolchain + native/audio/wasm env mirror the
          # crane build inputs so `cargo`/`dx` in the shell match `nix build`.
          # ============================================================
          devShells.default = pkgs.mkShell ({
            packages = with pkgs; [
              rustToolchain
              dioxus-cli
              wasm-bindgen-cli
              tailwindcss_4
              cargo-watch
              cargo-nextest
              bacon
              flac
            ]
            ++ lib.optionals pkgs.stdenv.isLinux [
              # pw-jack — run the live rigs through PipeWire's JACK shim.
              pkgs.pipewire.jack
              # jack_iodelay / jack_lsp — measure real round-trip latency.
              pkgs.jack-example-tools
            ]
            ++ buildInputs
            ++ nativeBuildInputs;

            shellHook = ''
              [ -f .env ] && { set -a; source .env; set +a; }
              # Prefer a cargo-installed dx so we track the FTS-pinned Dioxus
              # release (matches session / the monorepo). nixpkgs' dioxus-cli
              # lags; the shared dioxus-flake keeps the toolchain in lockstep.
              export PATH="$HOME/.cargo/bin:$PATH"
              DX_VERSION=$(dx --version 2>/dev/null | grep -oE 'dioxus [0-9.]+' | awk '{print $2}' || echo "0")
              if [ "$DX_VERSION" != "0.7.9" ]; then
                echo "  Installing dx 0.7.9..."
                cargo install dioxus-cli --locked --version "=0.7.9" 2>/dev/null || \
                  cargo install --git https://github.com/DioxusLabs/dioxus dioxus-cli --locked
              fi

              # Workspace shortcuts (replace the old devenv `scripts`).
              alias signal-build='cargo build --workspace'
              alias signal-check='cargo clippy --workspace -- -D warnings'
              alias signal-test='cargo nextest run --workspace'

              echo ""
              echo "  Signal dev shell"
              echo "  ────────────────────────────────────────"
              echo "  signal-build  — cargo build --workspace"
              echo "  signal-check  — clippy (warnings-as-errors)"
              echo "  signal-test   — cargo nextest run --workspace"
              echo ""
              echo "  Rust: $(rustc --version)"
              echo "  dx:   $(dx --version 2>/dev/null || echo 'not available')"
              echo ""
            '';
          }
          // {
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
            # WebKitGTK accelerated compositing fails on NixOS (GBM buffer error → white window).
            # Disabling it forces software rendering. See: https://github.com/NixOS/nixpkgs/issues/32580
            WEBKIT_DISABLE_COMPOSITING_MODE = "1";
          }
          // lib.optionalAttrs pkgs.stdenv.isDarwin {
            DYLD_LIBRARY_PATH = libPath;
          });
        };
    };
}

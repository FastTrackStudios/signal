{
  description = "FastTrackStudio Session — session/project management domain";

  inputs = {
    dioxus-flake.url = "github:FastTrackStudios/Dioxus-Flake";
    nixpkgs.follows = "dioxus-flake/nixpkgs";
    flake-parts.url = "github:hercules-ci/flake-parts";
    rust-overlay = {
      follows = "dioxus-flake/rust-overlay";
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
    # Shared FTS repo-hygiene hub: pinned capn/tracey + the `cargo xtask ci`
    # battery, so session runs the same CI gate as every other FTS repo.
    fts-repo.url = "git+https://codeberg.org/FastTrackStudios/fts-repo";
    fts-repo.inputs.nixpkgs.follows = "nixpkgs";
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

  outputs = { self, flake-parts, crane, devenv, devenv-root, fts-flake, fts-repo, dioxus-flake, ... } @inputs:
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
              libGL vulkan-loader gtk3 glib
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

            # Session CLI
            session-cli = craneLib.buildPackage (commonArgs // {
              pname = "session-cli";
              version = rev;
              inherit cargoArtifacts;
              cargoExtraArgs = "-p session-cli";
              doCheck = false;
            });

            # Session Desktop App
            session-desktop = craneLib.buildPackage (commonArgs // {
              pname = "session-desktop";
              version = rev;
              inherit cargoArtifacts;
              buildPhaseCargoCommand = ''
                cd apps/desktop
                dx build --release --platform desktop
              '';
              installPhaseCommand = ''
                mkdir -p $out/Applications $out/bin
                if [ -d "apps/desktop/target/dx/session-desktop/release/macos" ]; then
                  cp -r apps/desktop/target/dx/session-desktop/release/macos/*.app $out/Applications/
                  ln -s "$out/Applications/"*.app"/Contents/MacOS/"* $out/bin/session-desktop
                elif [ -f "target/release/session-desktop" ]; then
                  cp target/release/session-desktop $out/bin/
                fi
              '';
              doCheck = false;
            });

            # Session Extension — SHM guest process for REAPER integration
            session-extension = craneLib.buildPackage (commonArgs // {
              pname = "session-extension";
              version = rev;
              inherit cargoArtifacts;
              cargoExtraArgs = "-p session-extension";
              doCheck = false;
            });

            default = self'.packages.session-desktop;
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
              session-build.exec = "cargo build --workspace";
              session-build.description = "Build entire workspace";

              session-check.exec = "cargo clippy --workspace -- -D warnings";
              session-check.description = "Run clippy with warnings-as-errors";

              session-test.exec = "cargo nextest run --workspace";
              session-test.description = "Run all unit tests";
            };

            enterShell = ''
              [ -f .env ] && { set -a; source .env; set +a; }
              export PATH="$HOME/.cargo/bin:$PATH"
              DX_VERSION=$(dx --version 2>/dev/null | grep -oE 'dioxus [0-9.]+' | awk '{print $2}' || echo "0")
              if [ "$DX_VERSION" != "0.7.9" ]; then
                echo "  Installing dx 0.7.9..."
                cargo install dioxus-cli --locked --version "=0.7.9"
              fi
              echo ""
              echo "  Session dev shell (devenv)"
              echo "  ────────────────────────────────────────"
              echo "  session-build  — cargo build --workspace"
              echo "  session-check  — clippy (warnings-as-errors)"
              echo "  session-test   — cargo nextest run --workspace"
              echo ""
              echo "  Rust: $(rustc --version)"
              echo "  dx:   $(dx --version 2>/dev/null || echo 'not available')"
              echo ""
            '';
          };

          # ── CI shell — `cargo xtask ci` ──────────────────────────────
          # Slim shell carrying the build/system deps needed to COMPILE the
          # workspace plus the shared FTS hygiene tooling (same pinned
          # versions every FTS repo uses). Reuses the default devenv shell's
          # environment via inputsFrom. CI runs:
          #   nix develop .#ci --impure --command cargo xtask ci
          # Self-contained CI shell from fts-repo (rustup honors
          # rust-toolchain.toml + nextest/capn/tracey + blitz/GPU build deps).
          # NOT inputsFrom the devenv default shell — that doesn't put cargo
          # on PATH under `nix develop .#ci` (exec: cargo: not found).
          devShells.ci = fts-repo.lib.mkDevShell {
            inherit system;
            extraPackages = pkgs: fts-repo.lib.ftsUiBuildInputs pkgs;
          };
        };
    };
}

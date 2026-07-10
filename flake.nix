{
  description = "FastTrackStudio — one workspace: daw / session / signal / keyflow + THE app";

  inputs = {
    # Shared Dioxus toolchain hub — every FTS Dioxus repo follows its nixpkgs /
    # rust-overlay pins so `dx` and the Rust toolchain stay in lockstep.
    dioxus-flake.url = "github:FastTrackStudios/Dioxus-Flake";
    nixpkgs.follows = "dioxus-flake/nixpkgs";
    rust-overlay.follows = "dioxus-flake/rust-overlay";
    flake-parts.url = "github:hercules-ci/flake-parts";
  };

  nixConfig = {
    extra-trusted-public-keys = [
      "fasttrackstudio.cachix.org-1:r7v7WXBeSZ7m5meL6w0wttnvsOltRvTpXeVNItcy9f4="
    ];
    extra-substituters = [
      "https://fasttrackstudio.cachix.org"
    ];
  };

  outputs = { self, flake-parts, dioxus-flake, ... } @inputs:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" "x86_64-darwin" "aarch64-darwin" "aarch64-linux" ];

      perSystem = { self', config, pkgs, lib, system, ... }:
        let
          # Rust toolchain — the FTS-wide pin (same as the dissolved
          # signal/daw/session flakes), with wasm for the web remotes.
          rustToolchain = pkgs.rust-bin.stable."1.94.0".default.override {
            extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
            targets = [ "wasm32-unknown-unknown" ];
          };

          buildInputs = (with pkgs; [
            openssl openssl.dev libiconv pkg-config fontconfig freetype cmake python3
          ])
          ++ lib.optionals pkgs.stdenv.isLinux (with pkgs; [
            alsa-lib alsa-lib.dev
            # avahi — vox-discover links libavahi-client (mDNS service
            # discovery for the rig remotes); `.dev` carries the headers.
            avahi avahi.dev
            # JACK headers/lib for cpal's `jack` feature (signal-sampler).
            # At runtime, run under `pw-jack` to route through PipeWire.
            libjack2
            # libpipewire headers/lib for cpal's native `pipewire` feature —
            # talks to PipeWire directly (no JACK shim). `.dev` carries the
            # `libpipewire-0.3.pc` + headers that bindgen/pkg-config need.
            pipewire pipewire.dev
            glib gtk3 gdk-pixbuf pango cairo atk harfbuzz
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

          libPath = lib.makeLibraryPath (with pkgs;
            [ fontconfig freetype openssl ]
            ++ lib.optionals pkgs.stdenv.isLinux [
              alsa-lib avahi libjack2 pipewire
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
          # Dev Shell — one shell for the whole workspace. Toolchain +
          # native/audio/wasm env so `cargo` / `dx` behave identically
          # for the rig (rigd), the web remotes, and THE app.
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
              # release. nixpkgs' dioxus-cli lags; the shared dioxus-flake
              # keeps the toolchain in lockstep.
              export PATH="$HOME/.cargo/bin:$PATH"
              DX_VERSION=$(dx --version 2>/dev/null | grep -oE 'dioxus [0-9.]+' | awk '{print $2}' || echo "0")
              if [ "$DX_VERSION" != "0.7.9" ]; then
                echo "  Installing dx 0.7.9..."
                cargo install dioxus-cli --locked --version "=0.7.9" 2>/dev/null || \
                  cargo install --git https://github.com/DioxusLabs/dioxus dioxus-cli --locked
              fi

              echo ""
              echo "  FastTrackStudio dev shell"
              echo "  ─────────────────────────────────────────────"
              echo "  cargo check --workspace --exclude vox-discover"
              echo "  cargo build -p fasttrackstudio — THE app (--engine = headless rig)"
              echo "  (cd apps/fasttrackstudio && dx build --platform web --no-default-features --features signal)"
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
            # Unwrapped clang: the nix cc-wrapper injects hardening flags
            # (-fzero-call-used-regs) unsupported on wasm32 and leaks glibc
            # includes past -nostdlibinc (breaks ring). Builtin headers come
            # from the wrapper's resource-root instead.
            CC_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.clang-unwrapped}/bin/clang";
            AR_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.bintools}/bin/llvm-ar";
            CFLAGS_wasm32_unknown_unknown = "-isystem ${pkgs.llvmPackages_18.clang}/resource-root/include";
            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
          }
          // lib.optionalAttrs pkgs.stdenv.isLinux {
            LD_LIBRARY_PATH = libPath;
            XDG_DATA_DIRS = "${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}";
            # WebKitGTK accelerated compositing fails on NixOS (GBM buffer
            # error → white window). Force software rendering.
            # See: https://github.com/NixOS/nixpkgs/issues/32580
            WEBKIT_DISABLE_COMPOSITING_MODE = "1";
          }
          // lib.optionalAttrs pkgs.stdenv.isDarwin {
            DYLD_LIBRARY_PATH = libPath;
          });
        };
    };
}

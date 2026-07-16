{
  description = "FastTrackStudio — one workspace: daw / session / signal / keyflow + THE app";

  inputs = {
    # Shared Dioxus toolchain hub — every FTS Dioxus repo follows its nixpkgs /
    # rust-overlay pins so `dx` and the Rust toolchain stay in lockstep.
    dioxus-flake.url = "github:FastTrackStudios/Dioxus-Flake";
    nixpkgs.follows = "dioxus-flake/nixpkgs";
    rust-overlay.follows = "dioxus-flake/rust-overlay";
    flake-parts.url = "github:hercules-ci/flake-parts";

    # crane — cargo-in-nix builds for the deployable images (task-server
    # + the dx web bundles). Same pin style the dissolved task flake used.
    crane.url = "github:ipetkov/crane";

    # Dedicated, current-unstable nixpkgs used ONLY to source `dx`
    # (dioxus-cli) at the version the workspace Cargo.lock pins (0.7.9)
    # plus binaryen 129 (the wasm-opt dx 0.7.9 expects). The main
    # `nixpkgs` (dioxus-flake's pin) carries dioxus-cli 0.7.4 / binaryen
    # 126, which dx rejects / SIGABRTs with. Ported from the dissolved
    # task flake — see its nixpkgs-dx note.
    nixpkgs-dx.url = "github:NixOS/nixpkgs/d99b013d5d1931ad77fe3912ed218170dec5d9a4";
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
            # ONNX Runtime for Chatterbox TTS (session-guide/tts). `ort` uses
            # load-dynamic, so it dlopens libonnxruntime.so at runtime via
            # ORT_DYLIB_PATH (below) — never a downloaded binary, which Nix
            # rejects.
            onnxruntime
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

          # ============================================================
          # Deployable packages + OCI images (ported from the dissolved
          # task flake — task/flake.nix @ 69605cd28 — paths adjusted to
          # the monorepo: apps/web → apps/task/web, ui-lab →
          # apps/task/ui-lab; plus a NEW fts-site image for apps/site).
          # ============================================================

          # dx 0.7.9 + binaryen 129 + a modern crate fetcher — see the
          # nixpkgs-dx input note.
          pkgsDx = import inputs.nixpkgs-dx { inherit system; };
          dioxus-cli-79 = pkgsDx.dioxus-cli;

          craneLib = (inputs.crane.mkLib pkgs).overrideToolchain rustToolchain;

          # The whole monorepo is the build source: ONE workspace, so the
          # root Cargo.lock drives vendoring, and the [patch.crates-io]
          # path patches (libs/vendor/styx-format, libs/editor/vendor/
          # mermaid-rs-renderer) resolve in-tree. Keep the filter minimal
          # — the flake source is already the tracked git tree (no
          # target/, no untracked junk); we only strip obvious non-build
          # dirs to cut store-copy churn.
          ftsSrc = lib.cleanSourceWith {
            src = ./.;
            filter = path: type:
              let name = builtins.baseNameOf (toString path); in
              !(builtins.elem name [ "target" "node_modules" ".git" "result" "web-dist" ]);
          };

          # Vendor against the root Cargo.lock, with one substitution:
          # the lock pins `baseview` from Codys-Wright/baseview.git
          # (nice-plug-dioxus's VST windowing dep) — that repo is GONE
          # upstream, so the default builtins.fetchGit fails. The exact
          # commit is still reachable through GitHub's fork network via
          # RustAudio/baseview (SHA-in-want), so fetch the identical
          # tree from there. Content is verified by hash + the same
          # commit id, and cargo's vendor checksums still match.
          cargoVendorDir = craneLib.vendorCargoDeps {
            src = ftsSrc;
            # The override swaps the SOURCE of crane's package-extraction
            # derivation (drv extracts baseview-<ver> subdirs from the
            # checkout); returning a raw checkout would skip extraction.
            overrideVendorGitCheckout = ps: drv:
              if lib.any (p: lib.hasInfix "Codys-Wright/baseview" (p.source or "")) ps
              then drv.overrideAttrs (_: {
                src = pkgs.fetchgit {
                  url = "https://github.com/RustAudio/baseview.git";
                  rev = "00e438ff34f7e282776284e75b490a6fc36b16a7";
                  hash = "sha256-MeCvk/icQlEYaYZbayDx4S49QLRjrpMBDCzXe14VxW0=";
                };
              })
              else drv;
          };

          commonArgs = {
            src = ftsSrc;
            inherit cargoVendorDir;
            strictDeps = true;
            nativeBuildInputs = with pkgs; [ pkg-config ];
            buildInputs = with pkgs; [ openssl ];
          };

          # wasm-bindgen-cli matching the workspace Cargo.lock's
          # wasm-bindgen (0.2.126) — dx 0.7.9 rejects a mismatch. Built
          # through pkgsDx (its fetchCargoVendor pulls from
          # static.crates.io; the older pin's fetcher 403s).
          wasm-bindgen-cli-lock = pkgsDx.rustPlatform.buildRustPackage rec {
            pname = "wasm-bindgen-cli";
            version = "0.2.126";
            src = pkgsDx.fetchCrate {
              inherit pname version;
              hash = "sha256-H6Is3fiZVxZCfOMWK5dWMSrtn50VGv0sfdnsT+cTtyk=";
            };
            cargoHash = "sha256-VucqkXbCi4qtQzY/HrXiDnbSURsagPsdNVMn1Tw3UiY=";
            nativeBuildInputs = [ pkgsDx.pkg-config ];
            buildInputs = lib.optionals pkgsDx.stdenv.isLinux [ pkgsDx.openssl ]
              ++ lib.optionals pkgsDx.stdenv.isDarwin
                (with pkgsDx.darwin.apple_sdk.frameworks; [ Security CoreFoundation ]);
            doCheck = false;
          };

          # task-server, built from the monorepo workspace in ONE
          # derivation (cargoArtifacts = null skips crane's deps-only
          # split — mkDummySrc over a ~160-member workspace with custom
          # build.rs files is not worth the fragility).
          task-server = craneLib.buildPackage (commonArgs // {
            pname = "task-server";
            version = "0.1.0";
            cargoArtifacts = null;
            cargoExtraArgs = "--package task-server";
            doCheck = false;
          });

          # Shared env for the dx web-bundle builds: arborium /
          # arborium-tree-sitter compile the tree-sitter C runtime +
          # grammars to wasm via cc::Build — needs an unwrapped clang
          # targeting wasm32 and llvm-ar (the cc-wrapper injects
          # host-only hardening flags clang rejects for wasm). Without
          # these the C symbols stay as unresolved `(import "env" ...)`
          # entries and the shipped bundle white-screens. Mirrors
          # devShells.default exactly.
          dxWebEnv = {
            CC_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.clang-unwrapped}/bin/clang";
            AR_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.bintools-unwrapped}/bin/llvm-ar";
            CFLAGS_wasm32_unknown_unknown = "-isystem ${pkgs.llvmPackages_18.clang}/resource-root/include";
            # Hermetic dx: no network in the sandbox. With NO_DOWNLOADS
            # set, dx resolves wasm-opt / wasm-bindgen from PATH instead
            # of fetching them from GitHub.
            NO_DOWNLOADS = "1";
          };

          dxWebNativeInputs = commonArgs.nativeBuildInputs ++ [
            dioxus-cli-79            # dx 0.7.9 (nixpkgs-dx)
            wasm-bindgen-cli-lock    # 0.2.126, matches the lock
            pkgsDx.binaryen          # wasm-opt 129 — what dx 0.7.9 pins
          ] ++ (with pkgs; [
            tailwindcss_4
            # Pre-compression for --compression-static serving.
            brotli
            llvmPackages_18.clang-unwrapped
            llvmPackages_18.bintools-unwrapped
          ]);

          # dx bundle → $out/www (+ brotli pre-compression). The dx build
          # runs from the app dir but writes to the WORKSPACE-ROOT
          # target/dx/<name>/release/web/public.
          mkDxWebBundle = { pname, appDir, dxName, preBuild ? "" }:
            craneLib.buildPackage (commonArgs // dxWebEnv // {
              inherit pname;
              version = "0.1.0";
              cargoArtifacts = null;
              cargoExtraArgs = "--manifest-path ${appDir}/Cargo.toml";
              nativeBuildInputs = dxWebNativeInputs;
              doNotPostBuildInstallCargoBinaries = true;
              buildPhaseCargoCommand = ''
                export HOME="$TMPDIR/dx-home"
                mkdir -p "$HOME"
                ${preBuild}
                cd ${appDir}
                # --debug-symbols false: drop DWARF for a smaller release
                # bundle (and it sidesteps DWARF-version mismatches in
                # wasm-opt).
                dx build --release --platform web --debug-symbols false
              '';
              # buildPhase ends inside ${appDir}; anchor the copy at the
              # workspace root explicitly.
              installPhaseCommand = ''
                mkdir -p $out/www
                srcdir="$(pwd)"
                case "$srcdir" in */${appDir}) srcdir="''${srcdir%/${appDir}}";; esac
                cp -R "$srcdir/target/dx/${dxName}/release/web/public/." $out/www/
                # Pre-compress text/wasm so static-web-server's
                # --compression-static serves .br variants (the multi-MB
                # wasm goes over the wire at brotli size).
                find $out/www -type f \( -name '*.wasm' -o -name '*.js' \
                  -o -name '*.css' -o -name '*.html' -o -name '*.json' \
                  -o -name '*.svg' \) -exec brotli --keep --quality=9 {} +
              '';
              doCheck = false;
            });

          task-webapp = mkDxWebBundle {
            pname = "task-webapp";
            appDir = "apps/task/web";
            dxName = "task-app-web";
            preBuild = ''
              tailwindcss -i apps/task/web/tailwind.css -o apps/task/web/assets/tailwind.css
            '';
          };

          # fasttrackstudio.app website — `just site-build` is a plain
          # `dx build --platform web --release` (assets/tailwind.css is
          # committed; no tailwind step).
          fts-site-web = mkDxWebBundle {
            pname = "fts-site-web";
            appDir = "apps/site";
            dxName = "fts-site";
          };

          # ── ui-lab (pnpm + Vite) ─────────────────────────────────────
          # Its own pnpm workspace under apps/task/ui-lab (vendor/* holds
          # the vendored @bearcove/vox-* TS runtime as workspace:* deps).
          # Fetcher, config hook, and build pnpm MUST be the same major —
          # pin all three to pnpm_9.
          ui-lab = pkgs.stdenv.mkDerivation (finalAttrs: {
            pname = "task-ui-lab";
            version = "0.0.0";
            src = ./apps/task/ui-lab;
            nativeBuildInputs = [
              pkgs.nodejs_22
              pkgs.pnpm_9
              pkgs.pnpm_9.configHook
            ];
            pnpmDeps = pkgs.pnpm_9.fetchDeps {
              inherit (finalAttrs) pname version src;
              fetcherVersion = 2;
              hash = "sha256-JBWJhg81dixFwSc8GZg0yJcSyd38pR08VLcH81KkId4=";
            };
            buildPhase = ''
              runHook preBuild
              pnpm build
              runHook postBuild
            '';
            installPhase = ''
              runHook preInstall
              mkdir -p $out/www
              cp -R dist/* $out/www/
              runHook postInstall
            '';
          });

          # ── OCI container images (pure Nix, no Docker daemon) ────────
          # `dockerTools.streamLayeredImage` produces an *executable*
          # that streams a docker-archive tarball to stdout:
          #   $(nix build --print-out-paths .#task-server-image) \
          #     | skopeo copy docker-archive:/dev/stdin docker://…
          # Linux-only (guarded with lib.optionalAttrs below).

          # Git rev baked into the deployable images so a running
          # deployment can say WHICH commit it serves (version.json /
          # TASK_BUILD_REV). Only the cheap wrapper layers depend on it —
          # the expensive cargo/wasm derivations stay rev-free and cached.
          buildRev = self.rev or self.dirtyRev or "unknown";

          # static-web-server image factory: serve $root on :8080,
          # SPA-fallback unknown paths to index.html. HTML is `no-cache`
          # (nix-store mtimes are 1970 — heuristic freshness would pin
          # index.html for months); /assets/** is immutable-forever (dx
          # content-hashes asset URLs). Real copy, not symlinks:
          # static-web-server denies files resolving outside its root.
          mkStaticSite = { name, tag ? "latest", siteRoot }:
            let
              versionedRoot = pkgs.runCommand "${name}-root" { } ''
                mkdir -p $out
                cp -a ${siteRoot}/. $out/
                chmod u+w $out
                echo '{"rev":"${buildRev}"}' > $out/version.json
              '';
              swsConfig = pkgs.writeText "sws.toml" ''
                [general]
                host = "0.0.0.0"
                port = 8080
                root = "${versionedRoot}"
                page-fallback = "${versionedRoot}/index.html"
                log-level = "info"
                compression-static = true

                [advanced]
                [[advanced.headers]]
                source = "/**"
                [advanced.headers.headers]
                Cache-Control = "no-cache"

                [[advanced.headers]]
                source = "/assets/**"
                [advanced.headers.headers]
                Cache-Control = "public, max-age=31536000, immutable"
              '';
            in
            pkgs.dockerTools.streamLayeredImage {
              inherit name tag;
              contents = [ pkgs.static-web-server pkgs.cacert ];
              config = {
                Entrypoint = [ "/bin/static-web-server" ];
                Cmd = [ "--config-file" "${swsConfig}" ];
                ExposedPorts = { "8080/tcp" = { }; };
                Env = [ "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt" ];
              };
            };

          task-server-image = pkgs.dockerTools.streamLayeredImage {
            name = "task-server";
            tag = "latest";
            # git + curl: the snapshot engine shells out to them; cacert
            # for outbound TLS; yt-dlp for the watch-view transcript
            # ingest. /data is the TASK_DATA_ROOT volume.
            contents = with pkgs; [
              task-server
              git
              curl
              cacert
              bashInteractive
              coreutils
              yt-dlp
            ];
            extraCommands = ''
              mkdir -p data tmp
            '';
            config = {
              Entrypoint = [ "/bin/task-server" ];
              Env = [
                "TASK_BUILD_REV=${buildRev}"
                "TASK_DATA_ROOT=/data"
                "TASK_SERVER_BIND=0.0.0.0:8080"
                "RUST_LOG=info"
                "SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
                "GIT_SSL_CAINFO=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
                "PATH=/bin"
              ];
              ExposedPorts = { "8080/tcp" = { }; };
              Volumes = { "/data" = { }; };
              WorkingDir = "/data";
            };
          };

          task-web-image = mkStaticSite {
            name = "task-web";
            siteRoot = "${task-webapp}/www";
          };

          ui-lab-image = mkStaticSite {
            name = "task-ui-lab";
            siteRoot = "${ui-lab}/www";
          };

          fts-site-image = mkStaticSite {
            name = "fts-site";
            siteRoot = "${fts-site-web}/www";
          };

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

          # Env every dev/CI shell needs — build-script and bindgen
          # paths, the wasm cross toolchain, runtime library paths.
          # Shared by devShells.{default,ci,reaper-test}.
          commonShellEnv = {
            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
            OPENSSL_DIR = "${pkgs.openssl.dev}";
            OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
            # Unwrapped clang: the nix cc-wrapper injects hardening flags
            # (-fzero-call-used-regs) unsupported on wasm32 and leaks glibc
            # includes past -nostdlibinc (breaks ring). Builtin headers come
            # from the wrapper's resource-root instead.
            CC_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.clang-unwrapped}/bin/clang";
            # bintools (the wrapper) only exposes unprefixed names (ar, ld…);
            # llvm-ar lives in bintools-unwrapped. The wrapper path stood
            # here before and only "worked" while a warm target/ kept ring's
            # build script from re-running — cold CI builds hit it.
            AR_wasm32_unknown_unknown = "${pkgs.llvmPackages_18.bintools-unwrapped}/bin/llvm-ar";
            CFLAGS_wasm32_unknown_unknown = "-isystem ${pkgs.llvmPackages_18.clang}/resource-root/include";
            RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
          }
          // lib.optionalAttrs pkgs.stdenv.isLinux {
            LD_LIBRARY_PATH = libPath;
            # Chatterbox TTS: `ort` (load-dynamic) dlopens this exact .so at
            # runtime. Missing/unset → synthesis fails and section cues fall
            # back to the synth chime; the app still runs.
            ORT_DYLIB_PATH = "${pkgs.onnxruntime}/lib/libonnxruntime.so";
            XDG_DATA_DIRS = "${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}";
            # WebKitGTK accelerated compositing fails on NixOS (GBM buffer
            # error → white window). Force software rendering.
            # See: https://github.com/NixOS/nixpkgs/issues/32580
            WEBKIT_DISABLE_COMPOSITING_MODE = "1";
          }
          // lib.optionalAttrs pkgs.stdenv.isDarwin {
            DYLD_LIBRARY_PATH = libPath;
          };
        in {
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ inputs.rust-overlay.overlays.default ];
            config.allowUnfree = true;
          };

          formatter = pkgs.nixfmt-rfc-style;

          # ============================================================
          # Packages — deployable artifacts. The OCI images are
          # Linux-only (dockerTools needs a Linux store-path layout).
          # Names/tags match the old task flake so the registry/chart
          # contract holds: task-server / task-web / task-ui-lab (:latest)
          # + the new fts-site.
          # ============================================================
          packages = {
            inherit task-server task-webapp fts-site-web ui-lab;
          } // lib.optionalAttrs pkgs.stdenv.isLinux {
            inherit task-server-image task-web-image fts-site-image;
            # CI's contract name (the image itself is `task-ui-lab`).
            task-ui-lab-image = ui-lab-image;
          };

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
              # uv — vehicle for the graphify bootstrap below (graphify is a
              # PyPI tool, not in nixpkgs; python3 comes from buildInputs).
              uv
            ]
            ++ lib.optionals pkgs.stdenv.isLinux [
              # pw-jack — run the live rigs through PipeWire's JACK shim.
              pkgs.pipewire.jack
              # jack_iodelay / jack_lsp — measure real round-trip latency.
              pkgs.jack-example-tools
              # Xvfb + xvfb-run — virtual display for the REAPER
              # integration-test harness (`just reaper-integration-test`).
              # xvfb-run wraps xorg.xorgserver's Xvfb; Xvfb itself is added
              # so the harness can also spawn a raw virtual display.
              pkgs.xorg.xorgserver
              pkgs.xvfb-run
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

              # graphify — whole-repo knowledge-graph tool (safishamsi/graphify)
              # so any agent/assistant can understand this 160-crate tree without
              # grepping it cold. Not in nixpkgs; bootstrapped via uv like dx,
              # pinned for reproducibility. Code extraction is 100% local
              # (tree-sitter, no API calls); the [mcp] extra lets `graphify serve`
              # expose the graph over MCP. uv tools shim into ~/.local/bin.
              export PATH="$HOME/.local/bin:$PATH"
              # Dev-convenience installers (graphify, tracey) are for
              # interactive shells ONLY. In CI they're dead weight — and
              # worse: `cargo install tracey` compiled from source (or hung
              # on flaky crates.io egress, stderr silenced) on EVERY job,
              # stalling `nix develop -c true` for hours before the first
              # cargo step. Forgejo/GitHub runners set CI=true.
              if [ -z "''${CI:-}" ]; then
              GRAPHIFY_VERSION="0.9.15"
              if ! uv tool list 2>/dev/null | grep -q "graphifyy v$GRAPHIFY_VERSION"; then
                echo "  Installing graphify $GRAPHIFY_VERSION..."
                UV_PYTHON_DOWNLOADS=never uv tool install --force \
                  --python "${pkgs.python3}/bin/python3" \
                  "graphifyy[mcp]==$GRAPHIFY_VERSION" >/dev/null 2>&1 || \
                  echo "  (graphify install failed — run: uv tool install 'graphifyy[mcp]==$GRAPHIFY_VERSION')"
              fi

              # tracey — spec-coverage CLI (crates.io). Version-pinned so the
              # requirement traceability in docs/spec/** is reproducible across
              # machines. Config: .config/tracey/config.styx.
              TRACEY_VERSION=$(tracey --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || echo "0")
              if [ "$TRACEY_VERSION" != "1.3.0" ]; then
                echo "  Installing tracey 1.3.0..."
                cargo install tracey --locked --version "=1.3.0" 2>/dev/null || true
              fi
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
              echo "  graphify: $(graphify --version 2>/dev/null || echo 'not available') — 'just graph' to (re)build the repo knowledge graph"
              echo ""
            '';
          }
          // commonShellEnv);

          # CI shell — the default shell minus every interactive
          # convenience. No shellHook installers (the dx / graphify /
          # tracey cargo-installs stalled CI jobs for HOURS compiling
          # from source on cache misses), no dioxus-cli / tailwind /
          # editor tooling (CI drives plain cargo), no .env sourcing.
          # Toolchain + native headers + the env the build scripts
          # need, nothing else. Workflows enter it via
          # `nix develop .#ci`.
          devShells.ci = pkgs.mkShell ({
            packages = [ rustToolchain pkgs.cargo-nextest ]
            ++ buildInputs
            ++ [ pkgs.pkg-config pkgs.rustPlatform.bindgenHook ];

            # Seeded cargo-home bins (dx for the web-bundle steps)
            # resolve from PATH — never installed from here.
            shellHook = ''
              export PATH="$HOME/.cargo/bin:$PATH"
            '';
          }
          // commonShellEnv);

          # REAPER-integration shell — `.#ci` plus what the REAPER
          # harness needs around the pinned binary the workflow
          # resolves (jack routing + a virtual display). Workflows
          # enter it via `nix develop .#reaper-test`.
          devShells.reaper-test = pkgs.mkShell ({
            packages = [ rustToolchain pkgs.cargo-nextest ]
            ++ lib.optionals pkgs.stdenv.isLinux [
              # pw-jack + jack tools — the suites route audio through
              # PipeWire's JACK shim on the runner.
              pkgs.pipewire.jack
              pkgs.jack-example-tools
              # Xvfb + xvfb-run — the REAPER harness needs a display.
              pkgs.xorg.xorgserver
              pkgs.xvfb-run
            ]
            ++ buildInputs
            ++ [ pkgs.pkg-config pkgs.rustPlatform.bindgenHook ];

            shellHook = ''
              export PATH="$HOME/.cargo/bin:$PATH"
            '';
          }
          // commonShellEnv);
        };
    };
}

# dx web bundles (signal-web) + their static-site OCI images.
# dx bundle → $out/www (+ brotli pre-compression).
{ ... }:
{
  perSystem = { pkgs, lib, config, ... }:
    let
      inherit (config.fts) craneLib commonArgs mkStaticSite;

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

      dxWebNativeInputs = commonArgs.nativeBuildInputs ++ config.fts.nativeBuildInputs ++ [
        config.fts.dx.cli          # dx 0.7.9 (nixpkgs-dx)
        config.fts.dx.wasmBindgen  # 0.2.126, matches the lock
        config.fts.dx.binaryen     # wasm-opt 129 — what dx 0.7.9 pins
      ] ++ (with pkgs; [
        tailwindcss_4
        # Pre-compression for --compression-static serving.
        brotli
        llvmPackages_18.clang-unwrapped
        llvmPackages_18.bintools-unwrapped
        # Build-time tools for the HOST half of the dx build (see the
        # buildInputs comment on mkDxWebBundle). `strictDeps` keeps
        # buildInputs off the build-script PATH, so these have to be
        # declared native even though fts.buildInputs already lists them:
        # stylo's build.rs shells out to python3, and cmake backs several
        # of the Blitz-stack -sys crates.
        python3
        cmake
      ]);

      # The dx build runs from the app dir but writes to the
      # WORKSPACE-ROOT target/dx/<name>/release/web/public.
      mkDxWebBundle = { pname, appDir, dxName, preBuild ? "", dxArgs ? "" }:
        craneLib.buildPackage (commonArgs // dxWebEnv // {
          inherit pname;
          # `dx build --platform web` still compiles a HOST binary (the
          # server/SSG side), and signal-web now depends on the rig UI
          # crates. signal-guitar-ui reaches session-ui -> dynamic-template
          # -> daw -> daw-audio-io -> cpal -> {alsa,jack,pipewire}-sys, so
          # the host half of the build needs the native audio headers even
          # though the shipped artifact is wasm and never touches them.
          # Without them the derivation dies in a build script -- first
          # alsa-sys, then libspa-sys ("Package libpipewire-0.3 was not
          # found in the pkg-config search path").
          #
          # Take the devshell's own set rather than adding libraries one
          # failure at a time: it is already the curated list (alsa-lib.dev,
          # libjack2, pipewire.dev) and nativeBuildInputs brings the
          # bindgenHook that libspa-sys's bindgen run needs.
          buildInputs = (commonArgs.buildInputs or [ ]) ++ config.fts.buildInputs;
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
            dx build --release --platform web --debug-symbols false ${dxArgs}
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

      # task-webapp moved to the task repo with the August 2026 split,
      # along with task-server and the ui-lab bundle.

      # signal.fasttrackstudio.app — the Signal landing page, rig demos
      # and guide. No tailwind step: assets/site.css is committed and
      # inlined by the app, per the signal UI rule in CLAUDE.md.
      #
      # (This replaced an `apps/site` bundle for fasttrackstudio.app. That
      # directory does not exist in this repo — the site moved out with
      # the August 2026 split — so the derivation had been referring to
      # nothing and could not have built.)
      signal-web = mkDxWebBundle {
        pname = "signal-web";
        appDir = "apps/web";
        dxName = "signal-web";
        # --wasm-split is NOT optional here, and not really about splitting.
        #
        # Without it, wasm-opt (binaryen 129) aborts with SIGABRT and no
        # diagnostic on this bundle; dx reports the failure, carries on, and
        # ships the UNOPTIMISED wasm — 2,029,954 bytes. With it, wasm-opt
        # completes and the same bundle is 859,778. Measured both ways on a
        # clean target dir.
        #
        # The route splitting itself earns almost nothing (a 324-byte chunk):
        # the pages here are small and the CSS does the work. The flag is
        # load-bearing because of what it does to the wasm-opt pipeline, not
        # because of the chunking, so do not drop it as "we don't need
        # splitting" — that silently triples the bundle.
        #
        # `--ssg` pre-renders the guide. It builds the app's server as
        # well as its client, runs the server, asks it for the routes to
        # render (`signal_web::static_routes` — the router's static ones
        # plus every page of the guide vault) and requests each, which
        # writes it into the same `public` directory as an index.html.
        # Nothing deploys that server; it exists for the length of this
        # build. What ships is still a directory of static files.
        #
        # `--force-sequential` is REQUIRED alongside `--ssg`, and is not
        # about build speed. The pre-render borrows `public/index.html`
        # as its page shell, and the CLIENT build writes that file. Run
        # in parallel — the default — the server can reach the render
        # first, and every page comes out wrapped in Dioxus's bare
        # fallback shell: no <title>, no charset (so every em dash in the
        # prose is mojibake), and no bundle script, so nothing hydrates.
        # The failure is silent and the build still "succeeds".
        # (dioxus#3518.)
        #
        # The nix sandbox builds into a fresh directory, so the stale
        # cache that bites a local rebuild (`clear_cache(false)` serves a
        # route already in the cache rather than re-rendering it) cannot
        # happen here. `just web-build` deletes the directory for that.
        dxArgs = "--ssg --force-sequential --wasm-split --features wasm-split";
      };
    in
    {
      packages = { inherit signal-web; }
      // lib.optionalAttrs pkgs.stdenv.isLinux {
        signal-web-image = mkStaticSite {
          name = "signal-web";
          siteRoot = "${signal-web}/www";
        };
      };
    };
}

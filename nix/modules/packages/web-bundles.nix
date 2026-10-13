# dx web bundles (signal-web, session-web, session-app-web) + their
# static-site OCI images.
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

      dxWebNativeInputs = commonArgs.nativeBuildInputs ++ [
        config.fts.dx.cli          # dx 0.7.9 (nixpkgs-dx)
        config.fts.dx.wasmBindgen  # 0.2.126, matches the lock
        config.fts.dx.binaryen     # wasm-opt 129 — what dx 0.7.9 pins
      ] ++ (with pkgs; [
        tailwindcss_4
        # Resolves architect-ui's git-dep checkout for the tailwind
        # @source globs (session-web's preBuild).
        jq
        # Pre-compression for --compression-static serving.
        brotli
        # stylo (Blitz's CSS engine, in the Session app's page) generates
        # its property tables with a Python script at build time.
        python3
        llvmPackages_18.clang-unwrapped
        llvmPackages_18.bintools-unwrapped
      ]);

      # The dx build runs from the app dir but writes to the
      # WORKSPACE-ROOT target/dx/<name>/release/web/public.
      mkDxWebBundle = { pname, appDir, dxName, preBuild ? "", dxArgs ? "", env ? { } }:
        craneLib.buildPackage (commonArgs // dxWebEnv // env // {
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

      # The public demo's set: a live share link (`task share live <setlist>
      # --reset-minutes 5`) on the public Task, baked into the site's /demo —
      # days-to-praise's Worship Set, starting over every five minutes.
      sessionDemoLink = "https://task.fasttrackstudio.app/org/days-to-praise/share/1e75d7fa11c540ac8dbbd461048e59fc";

      # The Session app in a browser (`just web-daw`): what /demo and every
      # live link open, served under the site's /app/. A plain cargo +
      # wasm-bindgen build (no dx: the page is a library host, see
      # session-daw's web_host).
      session-app-web = craneLib.buildPackage (commonArgs // dxWebEnv // {
        pname = "session-app-web";
        version = "0.1.0";
        cargoArtifacts = null;
        nativeBuildInputs = dxWebNativeInputs;
        doNotPostBuildInstallCargoBinaries = true;
        buildPhaseCargoCommand = ''
          # `wasm-release`: size-optimized with whole-program LTO, the
          # audio path at full speed (see the profile in Cargo.toml).
          cargo build --profile wasm-release -p session-daw-web --target wasm32-unknown-unknown
          mkdir -p dist
          wasm-bindgen --target web --no-typescript --out-dir dist \
            target/wasm32-unknown-unknown/wasm-release/session-daw-web.wasm
          wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
            --enable-sign-ext --enable-mutable-globals --enable-reference-types \
            --enable-multivalue dist/session-daw-web_bg.wasm -o dist/session-daw-web_bg.wasm
        '';
        installPhaseCommand = ''
          mkdir -p $out/www
          cp -R dist/. $out/www/
          cp apps/session/daw-web/www/index.html $out/www/
          # The best brotli there is: done once per build, and the wasm
          # comes out 6.4 MB against 7.5 at Cloudflare's own level.
          find $out/www -type f \( -name '*.wasm' -o -name '*.js' -o -name '*.html' \) \
            -exec brotli --keep --quality=11 --lgwin=24 {} +
        '';
        doCheck = false;
      });

      # task-webapp moved to the task repo with the August 2026 split,
      # along with task-server and the ui-lab bundle.

      # signal.fasttrackstudio.app — the Signal landing page, rig demos
      # and guide (`just signal-web-build`). No tailwind step:
      # assets/site.css is committed and inlined by the app.
      #
      # (This replaced an `fts-site-web` bundle of `apps/site` for
      # fasttrackstudio.app — a directory that left with the August 2026
      # split, so that derivation referred to nothing.)
      signal-web = mkDxWebBundle {
        pname = "signal-web";
        appDir = "apps/signal/web";
        dxName = "signal-web";
        # --wasm-split is NOT optional here, and not really about splitting.
        #
        # Without it, wasm-opt (binaryen 129) aborts with SIGABRT and no
        # diagnostic on this bundle; dx reports the failure, carries on, and
        # ships the UNOPTIMISED wasm — 2,029,954 bytes. With it, wasm-opt
        # completes and the same bundle is 859,778. Measured both ways on a
        # clean target dir. The route splitting itself earns almost nothing;
        # do not drop the flag as "we don't need splitting" — that silently
        # triples the bundle.
        #
        # `--ssg` pre-renders the guide (`signal_web::static_routes`) into
        # the same `public` directory; `--force-sequential` is REQUIRED
        # alongside it (the pre-render borrows the client build's
        # `public/index.html` as its page shell — dioxus#3518).
        dxArgs = "--ssg --force-sequential --wasm-split --features wasm-split";
        env = {
          # `dx build --platform web` still compiles a HOST binary (the
          # SSG server), and signal-web depends on the rig UI crates,
          # which reach cpal -> {alsa,jack,pipewire}-sys: the host half
          # needs the devshell's native libraries even though the shipped
          # artifact is wasm.
          buildInputs = config.fts.buildInputs;
        };
      };

      # session.fasttrackstudio.app — landing page + the live in-browser
      # setlist demo (`just web`). apps/session/web/tailwind.css @sources
      # architect-ui, a GIT DEP with no stable path on disk — resolve it
      # via cargo metadata and symlink it in first, the same way
      # `just _web-tw-link` does locally. session-ui is a local path dep
      # (features/session/ui) and needs no symlink.
      session-web = mkDxWebBundle {
        pname = "session-web";
        appDir = "apps/session/web";
        dxName = "session-web";
        preBuild = ''
          mkdir -p apps/session/web/.tailwind-src
          for crate in architect-ui; do
            dir="$(cargo metadata --format-version 1 --offline \
              | jq -er --arg c "$crate" \
                  '.packages[] | select(.name == $c) | .manifest_path' \
              | head -1)"
            dir="''${dir%/Cargo.toml}"
            if [ -z "$dir" ] || [ ! -d "$dir" ]; then
              echo "cannot resolve $crate for the tailwind @source globs" >&2
              exit 1
            fi
            ln -sfn "$dir" "apps/session/web/.tailwind-src/$crate"
          done
          (cd apps/session/web && tailwindcss -i ./tailwind.css -o ./assets/tailwind.css --minify)
        '';
        # `--ssg` pre-renders the guide into the same `public` directory
        # the bundle lands in: dx builds the app's server too, runs it,
        # asks it for `static_routes` and requests each. Nothing deploys
        # that server — what ships is still a directory of static files.
        #
        # `--fullstack` because dx decides whether to build a server from
        # the CLIENT's features, and `dioxus/fullstack` is on the
        # `server` feature alone (its reqwest would be a second major in
        # the wasm binary). Without the flag there is no server target
        # and `--ssg` silently does nothing.
        #
        # `--force-sequential` because the pre-render borrows
        # `public/index.html` for its page shell and the client build
        # writes that file; in parallel the pages can come out in
        # Dioxus's bare fallback shell, with the build still reporting
        # success. (dioxus#3518.)
        dxArgs = "--ssg --fullstack --force-sequential";
        env = {
          SESSION_DEMO_LINK = sessionDemoLink;
          # The pre-render server is a native binary over the same session
          # crates the desktop links (Blitz, baseview's X11 window layer):
          # the desktop's native libraries, not just openssl.
          buildInputs = config.fts.buildInputs ++ [ pkgs.openssl ];
        };
      };

      # What session.fasttrackstudio.app serves: the site, and the app
      # under /app/.
      #
      # The app's page loads its module and wasm at URLs carrying the
      # wasm's hash. Their names never change, and Cloudflare gives `.js`
      # a four-hour browser TTL whatever the origin says: after a deploy a
      # returning browser paired its cached glue with the new wasm, which
      # does not link (a blank page). The page itself is `no-cache`, so a
      # new build's page always asks for its own files.
      session-site-root = pkgs.runCommand "session-site-root" { } ''
        mkdir -p $out/app
        cp -R ${session-web}/www/. $out/
        cp -R ${session-app-web}/www/. $out/app/
        chmod -R u+w $out/app
        v=$(sha256sum $out/app/session-daw-web_bg.wasm | cut -c1-16)
        # The snippets the module imports, preloaded beside it: their
        # directories are named by crate hash, so they are listed here from
        # what was built rather than written into the page.
        snippets=$(cd $out/app && find snippets -name '*.js' | sort \
          | sed 's|.*|    <link rel="modulepreload" href="./&" />|')
        substituteInPlace $out/app/index.html \
          --replace-fail '"./session-daw-web.js"' "\"./session-daw-web.js?v=$v\"" \
          --replace-fail 'href="./session-daw-web_bg.wasm"' "href=\"./session-daw-web_bg.wasm?v=$v\"" \
          --replace-fail '    <!-- snippets -->' "$snippets" \
          --replace-fail 'init();' "init({ module_or_path: \"./session-daw-web_bg.wasm?v=$v\" });"
        # Its precompressed copy is the old page: served first, it would
        # undo the above.
        rm -f $out/app/index.html.br $out/app/index.html.gz
      '';
    in
    {
      packages = { inherit signal-web session-web session-app-web; }
      // lib.optionalAttrs pkgs.stdenv.isLinux {
        signal-web-image = mkStaticSite {
          name = "signal-web";
          siteRoot = "${signal-web}/www";
        };
        session-web-image = mkStaticSite {
          name = "session-web";
          siteRoot = "${session-site-root}";
        };
      };
    };
}

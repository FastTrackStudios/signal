# dx web bundles (fts-site-web) + their static-site OCI images.
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
        # Pre-compression for --compression-static serving.
        brotli
        llvmPackages_18.clang-unwrapped
        llvmPackages_18.bintools-unwrapped
      ]);

      # The dx build runs from the app dir but writes to the
      # WORKSPACE-ROOT target/dx/<name>/release/web/public.
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

      # task-webapp moved to the task repo with the August 2026 split,
      # along with task-server and the ui-lab bundle.

      # fasttrackstudio.app website — `just site-build` is a plain
      # `dx build --platform web --release` (assets/tailwind.css is
      # committed; no tailwind step).
      fts-site-web = mkDxWebBundle {
        pname = "fts-site-web";
        appDir = "apps/site";
        dxName = "fts-site";
      };
    in
    {
      packages = { inherit fts-site-web; }
      // lib.optionalAttrs pkgs.stdenv.isLinux {
        fts-site-image = mkStaticSite {
          name = "fts-site";
          siteRoot = "${fts-site-web}/www";
        };
      };
    };
}

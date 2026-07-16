# crane plumbing shared by every deployable package (fills fts.craneLib,
# fts.src, fts.commonArgs, fts.buildRev).
{ inputs, self, ... }:
{
  perSystem = { pkgs, lib, config, ... }:
    let
      # The whole monorepo is the build source: ONE workspace, so the
      # root Cargo.lock drives vendoring, and the [patch.crates-io]
      # path patches (libs/vendor/styx-format, libs/editor/vendor/
      # mermaid-rs-renderer) resolve in-tree. Keep the filter minimal
      # — the flake source is already the tracked git tree (no
      # target/, no untracked junk); we only strip obvious non-build
      # dirs to cut store-copy churn.
      ftsSrc = lib.cleanSourceWith {
        src = ../..;
        filter = path: type:
          let name = builtins.baseNameOf (toString path); in
          !(builtins.elem name [ "target" "node_modules" ".git" "result" "web-dist" ]);
      };

      craneLib = (inputs.crane.mkLib pkgs).overrideToolchain config.fts.rustToolchain;

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
    in
    {
      fts.craneLib = craneLib;
      fts.src = ftsSrc;

      fts.commonArgs = {
        src = ftsSrc;
        inherit cargoVendorDir;
        strictDeps = true;
        nativeBuildInputs = with pkgs; [ pkg-config ];
        buildInputs = with pkgs; [ openssl ];
      };

      # Git rev baked into the deployable images so a running
      # deployment can say WHICH commit it serves (version.json /
      # TASK_BUILD_REV). Only the cheap wrapper layers depend on it —
      # the expensive cargo/wasm derivations stay rev-free and cached.
      fts.buildRev = self.rev or self.dirtyRev or "unknown";
    };
}

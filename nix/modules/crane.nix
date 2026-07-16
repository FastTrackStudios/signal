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

      # Vendor against the root Cargo.lock. (The old baseview fetch
      # override is gone: the dead Codys-Wright/baseview.git dep was
      # vendored into libs/vendor/baseview as a path dep 2026-07-16.)
      cargoVendorDir = craneLib.vendorCargoDeps { src = ftsSrc; };
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

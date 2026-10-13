# The Dioxus desktop GUI, built as an ordinary crane package —
# `nix run .#signal-desktop`. Not the notarized/bundled
# .app release artifact (that's still apps/desktop/ios/deploy-macos.sh
# via `dx build` for now); this is a plain native binary for local dev/rig
# use (cargoArtifacts = null — the workspace
# is too large/build.rs-heavy for crane's dummy-src deps split).
#
# `desktop`/`launch` are ordinary cargo features on non-wasm/non-ios targets
# (see apps/desktop/Cargo.toml), so no
# `dx` involvement is needed to produce a runnable binary — dx is only for
# the wasm/mobile/.app-bundle pipelines. `embed-web` is deliberately NOT
# enabled here yet: it needs apps/desktop/web-dist staged into the
# source before compiling (a separate `fts-web-dist` bundle, not built yet).
{ ... }:
{
  perSystem = { pkgs, lib, config, ... }:
    let
      # Reuse the toolchain's own GUI dep list (toolchain.nix already
      # assembled webkitgtk/gtk3/x11/vulkan for Linux, libiconv for
      # Darwin) instead of duplicating it — commonArgs.buildInputs alone
      # only carries openssl, which is enough for headless crates
      # (task-server) but not a wry/tao desktop window.
      guiArgs = {
        buildInputs = config.fts.buildInputs;
        # python3 explicitly: stylo's build.rs (nice-plug-dioxus/Blitz, a
        # transitive dep of the desktop GUI) shells out to `python3` and
        # needs it on PATH, not just linkable (same fix as fts-plugins in
        # reaper.nix).
        nativeBuildInputs = config.fts.nativeBuildInputs ++ [ pkgs.python3 ];
      };

      signal-desktop = config.fts.craneLib.buildPackage (config.fts.commonArgs // guiArgs // {
        pname = "signal-desktop";
        version = "0.0.2-alpha";
        cargoArtifacts = null;
        cargoExtraArgs = "--package signal-desktop";
        doCheck = false;
        meta.mainProgram = "signal-desktop";
      });

      # task-desktop moved to the task repo with the August 2026 split.
    in
    {
      packages = { inherit signal-desktop; };
    };
}

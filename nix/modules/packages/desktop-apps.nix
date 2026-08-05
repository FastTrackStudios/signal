# The two Dioxus desktop GUIs, built as ordinary crane packages —
# `nix run .#fasttrackstudio` / `.#task-desktop`. Not the notarized/bundled
# .app release artifact (that's still apps/fasttrackstudio/ios/deploy-macos.sh
# via `dx build` for now); this is a plain native binary for local dev/rig
# use, same shape as task-server.nix (cargoArtifacts = null — the workspace
# is too large/build.rs-heavy for crane's dummy-src deps split).
#
# `desktop`/`launch` are ordinary cargo features on non-wasm/non-ios targets
# (see apps/fasttrackstudio/Cargo.toml, apps/task/desktop/Cargo.toml), so no
# `dx` involvement is needed to produce a runnable binary — dx is only for
# the wasm/mobile/.app-bundle pipelines. `embed-web` is deliberately NOT
# enabled here yet: it needs apps/fasttrackstudio/web-dist staged into the
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

      fasttrackstudio = config.fts.craneLib.buildPackage (config.fts.commonArgs // guiArgs // {
        pname = "fasttrackstudio";
        version = "0.0.2-alpha";
        cargoArtifacts = null;
        cargoExtraArgs = "--package fasttrackstudio";
        doCheck = false;
        meta.mainProgram = "fasttrackstudio";
      });

      task-desktop = config.fts.craneLib.buildPackage (config.fts.commonArgs // guiArgs // {
        pname = "task-app-desktop";
        version = "0.1.0";
        cargoArtifacts = null;
        cargoExtraArgs = "--package task-app-desktop";
        doCheck = false;
        meta.mainProgram = "task-app-desktop";
      });
    in
    {
      packages = { inherit fasttrackstudio task-desktop; };
    };
}

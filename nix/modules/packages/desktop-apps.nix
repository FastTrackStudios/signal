# The Dioxus desktop apps, built as ordinary crane packages —
# `nix run .#session-desktop`, `nix run .#signal-desktop`. Not the
# notarized/bundled .app release artifacts (those are still
# apps/{session,signal}/desktop/ios/deploy-macos.sh
# via `dx build` for now); this is a plain native binary for local dev/rig
# use (cargoArtifacts = null — the workspace
# is too large/build.rs-heavy for crane's dummy-src deps split).
#
# `desktop`/`launch` are ordinary cargo features on non-wasm/non-ios targets
# (see the apps' Cargo.toml), so no
# `dx` involvement is needed to produce a runnable binary — dx is only for
# the wasm/mobile/.app-bundle pipelines. `embed-web` is deliberately NOT
# enabled on signal-desktop here yet: it needs apps/signal/desktop/web-dist
# staged into the source before compiling (`just signal-web-stage` does it
# locally; there is no nix bundle for it yet).
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

      session-desktop = config.fts.craneLib.buildPackage (config.fts.commonArgs // guiArgs // {
        pname = "session-desktop";
        version = "0.0.2-alpha";
        cargoArtifacts = null;
        cargoExtraArgs = "--package session-desktop";
        doCheck = false;
        meta.mainProgram = "session-desktop";
      });

      # The isolated live-rig app (one engine, rigs only).
      signal-desktop = config.fts.craneLib.buildPackage (config.fts.commonArgs // guiArgs // {
        pname = "signal-desktop";
        version = "0.0.2-alpha";
        cargoArtifacts = null;
        cargoExtraArgs = "--package signal-desktop";
        doCheck = false;
        meta.mainProgram = "signal-desktop";
      });
    in
    {
      packages = { inherit session-desktop signal-desktop; };
    };
}

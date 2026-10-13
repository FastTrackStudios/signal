{
  description = "Processor — the DSP, the effects, and the plugins.";

  # Deliberately a single file, unlike signal's dendritic nix/modules tree.
  # This repo builds Rust and nothing else: no web bundles, no REAPER config,
  # no deployable images. The shared toolchain hub is what matters — it is
  # what keeps the Rust pin and `dx` in lockstep across every FTS repo, so
  # a plugin built here is built by the same compiler as one built in signal.
  inputs = {
    dioxus-flake.url = "github:FastTrackStudios/Dioxus-Flake";
    nixpkgs.follows = "dioxus-flake/nixpkgs";
    rust-overlay.follows = "dioxus-flake/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
        # The workspace pin. Same as signal's, and it has to stay that way:
        # these crates are consumed from there as a git dep.
        toolchain = pkgs.rust-bin.stable."1.94.0".default.override {
          targets = [ "wasm32-unknown-unknown" ];
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          packages = [
            toolchain
            pkgs.just
            pkgs.cargo-nextest
            # stylo's build script generates its property tables with a
            # Python script, so the Blitz stack does not compile without it.
            pkgs.python3
          ]
            # The plugin faces render through wgpu, and the standalone host
            # opens a real window; on Linux that needs the audio and X11
            # headers. macOS gets them from the SDK.
            ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
              pkgs.pkg-config pkgs.alsa-lib pkgs.libjack2 pkgs.pipewire
              pkgs.xorg.libX11 pkgs.xorg.libXcursor pkgs.xorg.libxcb
              pkgs.libxkbcommon pkgs.vulkan-loader pkgs.mold
            ];
        };
      });
}

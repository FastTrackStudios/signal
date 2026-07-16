# REAPER-integration shell — `.#ci` plus what the REAPER harness needs
# around the pinned binary the workflow resolves (jack routing + a
# virtual display). Workflows enter it via `nix develop .#reaper-test`.
{ ... }:
{
  perSystem = { pkgs, lib, config, ... }: {
    devShells.reaper-test = pkgs.mkShell ({
      packages = [ config.fts.rustToolchain pkgs.cargo-nextest ]
      ++ lib.optionals pkgs.stdenv.isLinux [
        # pw-jack + jack tools — the suites route audio through
        # PipeWire's JACK shim on the runner.
        pkgs.pipewire.jack
        pkgs.jack-example-tools
        # Xvfb + xvfb-run — the REAPER harness needs a display.
        pkgs.xorg.xorgserver
        pkgs.xvfb-run
      ]
      ++ config.fts.buildInputs
      ++ [ pkgs.pkg-config pkgs.rustPlatform.bindgenHook ];

      shellHook = ''
        export PATH="$HOME/.cargo/bin:$PATH"
      '';
    }
    // config.fts.shellEnv);
  };
}

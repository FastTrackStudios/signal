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
        # A window manager. Not optional for GUI tests: a bare Xvfb
        # leaves windows unmanaged — nothing positions or stacks them,
        # and REAPER's restored dialogs cover whatever is under test, so
        # a screenshot shows nothing useful.
        pkgs.openbox
        # Screenshots (`import`, `identify`) and window lookup for
        # `daw::test::VirtualDisplay`.
        pkgs.imagemagick
        pkgs.xwininfo
        # Sending clicks and keys — closing REAPER's restored dialogs
        # and driving a panel.
        pkgs.xdotool
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

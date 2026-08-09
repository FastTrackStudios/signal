# REAPER-integration shell — `.#ci` plus what the REAPER harness needs
# around the pinned binary the workflow resolves (jack routing + a
# virtual display). Workflows enter it via `nix develop .#reaper-test`.
{ ... }:
{
  perSystem = { pkgs, lib, config, ... }: {
    devShells.reaper-test = pkgs.mkShell ({
      packages = [ config.fts.rustToolchain pkgs.cargo-nextest ]
      ++ lib.optionals pkgs.stdenv.isLinux [
        # REAPER itself — the same wrapped build `nix run .#fts-reaper`
        # uses, and the reason the shell exists rather than leaving the
        # harness to `which reaper`.
        #
        # The wrapping is the point: `resolve_gui_reaper_exe` takes the
        # first `reaper` on PATH, and an unwrapped one fails to dlopen
        # libxml2.so.2 inside SWELL. That failure surfaces as a GDK
        # assertion about a missing display, so it reads as a broken
        # Xvfb rather than a missing library and is easy to chase in the
        # wrong direction. The `reaper` package here pins libxml2_13,
        # whose soname matches what libSwell wants.
        config.packages.reaper
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

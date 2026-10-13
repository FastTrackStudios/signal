# FTS-Reaper: `nix run .#fts-reaper` — REAPER + SWS + ReaPack + every FTS
# plugin (CLAP+VST3), pre-wired into a config dir. No manual setup, no
# separate `fts-installer reaper` step.
#
# Built from the vendored reaper-flake recipes (nix/vendor/reaper-flake,
# subtree-imported — see wrapper/reaper/pkgs/{reaper,sws,reapack}.nix) plus a
# new `fts-plugins` crane package (this repo's own 17-plugin suite,
# apps/plugins/*, bundled the same way `just plugins-bundle` does).
#
# Plugin injection follows the exact idiom reaper-flake already used for
# SWS/ReaPack (idempotent launch-time symlinks into $REAPER_CONFIG/
# UserPlugins — see wrapper/reaper/pkgs/dmg.nix's fts-reaper-launcher),
# extended with a third category (VST3/CLAP) that didn't exist before.
{ ... }:
{
  perSystem = { pkgs, lib, config, ... }:
    let
      vendor = ../../vendor/reaper-flake;

      reaper = pkgs.callPackage (vendor + "/wrapper/reaper/pkgs/reaper.nix") {
        jackLibrary = pkgs.pipewire.jack;
        libxml2 = pkgs.libxml2_13; # .so.2 — matches nixpkgs' libSwell build
      };
      sws = pkgs.callPackage (vendor + "/wrapper/reaper/pkgs/sws.nix") { };
      reapack = pkgs.callPackage (vendor + "/wrapper/reaper/pkgs/reapack.nix") { };

      # The FTS plugin suite (bundler.toml's 17 CLAP+VST3 plugins),
      # bundled via fts-plugin-xtask — same recipe `just plugins-bundle`
      # runs, just inside the crane sandbox (offline: cargoVendorDir
      # already carries every crate). Single-arch only for now (whatever
      # `system` this perSystem is evaluating) — no lipo/universal step,
      # unlike the macOS release artifact.
      fts-plugins = config.fts.craneLib.buildPackage (config.fts.commonArgs // {
        pname = "fts-plugins";
        version = "0.1.0";
        cargoArtifacts = null;
        cargoExtraArgs = "--package fts-plugin-xtask";
        buildInputs = config.fts.buildInputs;
        # python3 explicitly: stylo's build.rs (nice-plug-dioxus/Blitz) shells
        # out to `python3` and needs it on PATH, not just linkable.
        nativeBuildInputs = config.fts.nativeBuildInputs ++ [ pkgs.python3 ];
        doNotPostBuildInstallCargoBinaries = true;
        doCheck = false;
        buildPhaseCargoCommand = ''
          for p in eq comp reverb delay tune modulation nam level saturate \
                   signal guide gate limiter trigger meter pitch unison; do
            cargo run -q -p fts-plugin-xtask -- bundle -p "$p-plugin" --release --offline
          done
        '';
        installPhaseCommand = ''
          mkdir -p $out
          cp -r target/bundled/. $out/
        '';
      });

      reaperConfig = "\${FTS_REAPER_CONFIG:-$HOME/fasttrackstudio}";

      # FTS default reaper.ini — general prefs only (audio driver mode,
      # undo memory, the reaper_fts_extensions dock layout, SWS loudness
      # targets, toolbar geometry). No machine-specific window positions
      # or audio/MIDI device selection — those stay on REAPER's own
      # auto-detect. Sourced from the actual production rig's config
      # (~/fasttrackstudio/reaper.ini), hardware-specific bits stripped.
      reaperIniTemplate = vendor + "/assets/reaper.ini.template";

      # This repo's versioned REAPER configuration — keybindings,
      # toolbars, mouse modifiers, FX tags/folders, screensets, the
      # active theme, and the ReaPack manifest. See
      # nix/reaper-config/README.md for what is and is not in here.
      #
      # ~4 MB, because ReaPack's ~994 downloaded scripts are NOT
      # versioned: `ReaPack/registry.db` is the manifest they are
      # restored from. First launch on a new machine therefore needs one
      # ReaPack "synchronise packages" to fetch them.
      ftsReaperConfig = ../../reaper-config;

      fts-reaper = pkgs.writeShellApplication {
        name = "fts-reaper";
        text = ''
          CONFIG_DIR="${reaperConfig}"
          mkdir -p "$CONFIG_DIR/UserPlugins" "$CONFIG_DIR/Scripts"

          # Never clobber a configured rig — only seed the FTS defaults
          # the first time this config dir is used.
          # install -m: the nix store source is read-only; REAPER needs to
          # write this file back on exit.
          [ -f "$CONFIG_DIR/reaper.ini" ] || install -m 644 "${reaperIniTemplate}" "$CONFIG_DIR/reaper.ini"

          # The versioned configuration. Copied (not symlinked) and made
          # writable: REAPER rewrites these files as you work, and a
          # symlink into the read-only nix store would make every
          # toolbar edit fail.
          #
          # Absolute paths inside reaper.ini were tokenised on export —
          # the active theme's path among them — so they are expanded to
          # this machine's config dir here. Without that, a config
          # exported on one machine points at a directory that does not
          # exist on the next.
          for f in "${ftsReaperConfig}"/*.ini "${ftsReaperConfig}"/*.db; do
            [ -e "$f" ] || continue
            install -m 644 "$f" "$CONFIG_DIR/$(basename "$f")"
          done
          for d in ColorThemes MenuSets TrackTemplates ProjectTemplates Configurations ReaPack; do
            if [ -d "${ftsReaperConfig}/$d" ]; then
              mkdir -p "$CONFIG_DIR/$d"
              cp -RL --no-preserve=mode "${ftsReaperConfig}/$d"/. "$CONFIG_DIR/$d/"
            fi
          done
          # Our own scripts and JSFX, alongside whatever ReaPack manages.
          for d in Scripts Effects; do
            if [ -d "${ftsReaperConfig}/$d" ]; then
              mkdir -p "$CONFIG_DIR/$d"
              cp -RL --no-preserve=mode "${ftsReaperConfig}/$d"/. "$CONFIG_DIR/$d/"
            fi
          done

          # Reuse a licence this machine already has.
          #
          # The key is personal and deliberately NOT versioned — putting
          # it in a public repo would be publishing it. But a machine
          # that already runs REAPER has one lying around, and making
          # someone re-enter it just because they launched a different
          # config dir is pointless friction. So: look in the usual
          # places, copy the first hit, and never overwrite one that is
          # already here.
          if [ ! -f "$CONFIG_DIR/reaper-license.rk" ]; then
            for candidate in \
              "''${FTS_REAPER_LICENSE:-}" \
              "$HOME/.config/REAPER/reaper-license.rk" \
              "$HOME/.config/reaper/reaper-license.rk" \
              "$HOME/fts-dev/reaper-license.rk" \
              "$HOME/.reaper/reaper-license.rk" \
              "$HOME/Library/Application Support/REAPER/reaper-license.rk"; do
              if [ -n "$candidate" ] && [ -f "$candidate" ]; then
                install -m 600 "$candidate" "$CONFIG_DIR/reaper-license.rk"
                echo "fts-reaper: reusing licence from $candidate" >&2
                break
              fi
            done
          fi

          # $REAPER_RESOURCES → this config dir.
          if grep -q 'REAPER_RESOURCES' "$CONFIG_DIR/reaper.ini" 2>/dev/null; then
            sed -i "s|\$REAPER_RESOURCES|$CONFIG_DIR|g" "$CONFIG_DIR/reaper.ini"
          fi

          ln -sf "${sws}"/UserPlugins/* "$CONFIG_DIR/UserPlugins/" 2>/dev/null || true
          ln -sf "${reapack}"/UserPlugins/* "$CONFIG_DIR/UserPlugins/" 2>/dev/null || true
          find "${fts-plugins}" \( -iname '*.vst3' -o -iname '*.clap' \) -maxdepth 3 -print0 \
            | xargs -0 -I{} ln -sf {} "$CONFIG_DIR/UserPlugins/"

          exec "${reaper}/bin/reaper" -cfgfile "$CONFIG_DIR/reaper.ini" -newinst "$@"
        '';
      };
    in
    {
      packages = { inherit reaper sws reapack fts-plugins fts-reaper; };
    };
}

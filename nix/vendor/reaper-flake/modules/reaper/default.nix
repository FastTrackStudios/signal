# REAPER NixOS module — dendritic option tree
#
# Option categories mirror the `reaper-config` crate in reaper-file:
#   settings.general      → General prefs     (general.rs)
#   settings.audio        → Audio prefs       (audio.rs)
#   settings.performance  → Perf prefs        (performance.rs)
#   settings.recording    → Record prefs      (recording.rs)
#   settings.appearance   → Appearance prefs  (appearance.rs)
#   settings.midi         → MIDI prefs        (midi.rs)
#   settings.editing      → Editing prefs     (editing.rs)
#   settings.automation   → Automation prefs  (automation.rs)
#   settings.extra        → Arbitrary [reaper] keys
#   settings.extraSections → Arbitrary extra INI sections
#
# Generated outputs (read-only, always available when enable = true):
#   programs.reaper.configText  — the INI as a string
#   programs.reaper.configFile  — the INI as a Nix store path

{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.programs.reaper;

  # ── INI serialisation helpers ──────────────────────────────────────────────

  # Render a single value to its INI string representation.
  # Booleans → "1" / "0"; everything else → toString.
  toIniValue =
    v: if builtins.isBool v then (if v then "1" else "0") else toString v;

  # Remove null-valued keys from an attrset.
  dropNulls = lib.filterAttrs (_: v: v != null);

  # Render a single INI section, e.g.:
  #   [reaper]
  #   audiodriver=1
  #   undomaxmem=0
  renderSection =
    name: kvs:
    let
      clean = dropNulls kvs;
    in
    lib.optionalString (clean != { }) (
      "[${name}]\n"
      + lib.concatStringsSep "\n" (lib.mapAttrsToList (k: v: "${k}=${toIniValue v}") clean)
      + "\n"
    );

  # ── Map typed options → INI key names ─────────────────────────────────────
  # All keys go into [reaper] (lowercase, as REAPER on Linux writes them).

  # general.*
  generalKvs = {
    lastproject = cfg.settings.general.lastProject;
    undomaxmem = cfg.settings.general.undoMaxMem;
    undoflags = cfg.settings.general.undoFlags;
    autosaveinterval = cfg.settings.general.autoSaveInterval;
  } // cfg.settings.general.extra;

  # audio.*
  audioKvs = {
    audiodriver = cfg.settings.audio.driver;
    audiocloseinactive = cfg.settings.audio.closeInactive;
    audioclosestop = cfg.settings.audio.closeWhenStopped;
    hwfadex = cfg.settings.audio.hardwareFade;
    optimizesilence = cfg.settings.audio.optimizeSilence;
    pdcautobypassms = cfg.settings.audio.pdcAutoBypassMs;
    csurfrate = cfg.settings.audio.controlSurfaceRate;
  } // cfg.settings.audio.extra;

  # performance.*
  performanceKvs = {
    autonbworkerthreads = cfg.settings.performance.autoWorkerThreads;
    workthreads = cfg.settings.performance.workThreads;
    workbufmsex = cfg.settings.performance.workBufferMs;
    workbuffxuims = cfg.settings.performance.workBufferFxUiMs;
    prebufperb = cfg.settings.performance.preBufferPercent;
    renderaheadlen = cfg.settings.performance.renderAheadLen;
    fxdenorm = cfg.settings.performance.fxDenorm;
  } // cfg.settings.performance.extra;

  # recording.*
  recordingKvs = cfg.settings.recording.extra;

  # appearance.*
  appearanceKvs = cfg.settings.appearance.extra;

  # midi.*
  midiKvs = cfg.settings.midi.extra;

  # editing.*
  editingKvs = cfg.settings.editing.extra;

  # automation.*
  automationKvs = cfg.settings.automation.extra;

  # Merge all [reaper]-section KVs; user's settings.extra wins on conflict.
  reaperSection =
    generalKvs
    // audioKvs
    // performanceKvs
    // recordingKvs
    // appearanceKvs
    // midiKvs
    // editingKvs
    // automationKvs
    // cfg.settings.extra;

  # All INI sections.
  allSections =
    lib.optionalAttrs (dropNulls reaperSection != { }) { reaper = reaperSection; }
    // cfg.settings.extraSections;

  # Final INI text.
  iniContent = lib.concatStringsSep "\n" (lib.mapAttrsToList renderSection allSections);

in
{
  # ── Option declarations ────────────────────────────────────────────────────

  options.programs.reaper = {

    enable = lib.mkEnableOption "REAPER DAW declarative configuration";

    settings = {

      # ── General ───────────────────────────────────────────────────────────

      general = {
        lastProject = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          description = ''
            `lastproject`: Path of the last opened project file.
            Set to `""` to have REAPER open a blank project on startup.
          '';
          example = "";
        };

        undoMaxMem = lib.mkOption {
          type = lib.types.nullOr lib.types.ints.unsigned;
          default = null;
          description = ''
            `undomaxmem`: Maximum undo-history memory in MB.
            `0` means unlimited.
          '';
          example = 512;
        };

        undoFlags = lib.mkOption {
          type = lib.types.nullOr lib.types.ints.unsigned;
          default = null;
          description = ''
            `undoflags`: Bitfield controlling what is included in undo history.
            - `&1`  include item selection changes
            - `&2`  include time selection changes
            - `&16` discard oldest state when full (default: discard newest)
          '';
          example = 3;
        };

        autoSaveInterval = lib.mkOption {
          type = lib.types.nullOr lib.types.ints.unsigned;
          default = null;
          description = ''
            `autosaveinterval`: Auto-save interval in minutes.
            `0` disables auto-save.
          '';
          example = 5;
        };

        extra = lib.mkOption {
          type = lib.types.attrsOf lib.types.anything;
          default = { };
          description = "Extra general key/value pairs for the `[reaper]` section.";
          example = {
            maxrecentprojects = 20;
          };
        };
      };

      # ── Audio ─────────────────────────────────────────────────────────────

      audio = {
        driver = lib.mkOption {
          type = lib.types.nullOr lib.types.ints.unsigned;
          default = null;
          description = ''
            `audiodriver`: Audio backend selection.
            - `0`  ASIO (Windows)
            - `1`  JACK (Linux / macOS via JACK bridge)
            - `2`  CoreAudio (macOS)
            - `3`  WaveOut (Windows)
            - `4`  WASAPI (Windows)
            - `5`  DirectSound (Windows)
          '';
          example = 1;
        };

        closeInactive = lib.mkOption {
          type = lib.types.nullOr lib.types.ints.unsigned;
          default = null;
          description = ''
            `audiocloseinactive`: Bitfield for automatic audio device closure.
            - `&1`  close when stopped and application is **inactive**
            - `&2`  close when stopped and application is **minimized**
          '';
          example = 0;
        };

        closeWhenStopped = lib.mkOption {
          type = lib.types.nullOr lib.types.ints.unsigned;
          default = null;
          description = ''
            `audioclosestop`: Close audio device when playback is stopped (less responsive).
            - `0`  keep device open
            - `1`  close when stopped
          '';
          example = 0;
        };

        hardwareFade = lib.mkOption {
          type = lib.types.nullOr lib.types.ints.unsigned;
          default = null;
          description = ''
            `hwfadex`: Tiny hardware fade-in / fade-out on playback transitions.
            - `&1`  fade-out on playback stop
            - `&2`  fade-in on playback start
            Default (factory): `3` (both on).
          '';
          example = 3;
        };

        optimizeSilence = lib.mkOption {
          type = lib.types.nullOr lib.types.ints.unsigned;
          default = null;
          description = ''
            `optimizesilence`: Bitfield for CPU / silence optimizations.
            - `&1`  reduce CPU use of silent tracks during playback (experimental)
            - `&2`  disable FX auto-bypass during offline render / apply FX
          '';
          example = 0;
        };

        pdcAutoBypassMs = lib.mkOption {
          type = lib.types.nullOr lib.types.numbers.nonnegative;
          default = null;
          description = ''
            `pdcautobypassms`: Auto-bypass PDC-affected tracks on record arm
            when PDC exceeds this threshold (ms). `0` disables.
          '';
          example = 0;
        };

        controlSurfaceRate = lib.mkOption {
          type = lib.types.nullOr lib.types.ints.unsigned;
          default = null;
          description = ''
            `csurfrate`: Control-surface update frequency in Hz. Default: `15`.
          '';
          example = 15;
        };

        extra = lib.mkOption {
          type = lib.types.attrsOf lib.types.anything;
          default = { };
          description = "Extra audio key/value pairs for the `[reaper]` section.";
        };
      };

      # ── Performance ───────────────────────────────────────────────────────

      performance = {
        autoWorkerThreads = lib.mkOption {
          type = lib.types.nullOr lib.types.bool;
          default = null;
          description = ''
            `autonbworkerthreads`: Auto-detect the number of audio worker threads.
            When `true`, REAPER picks based on CPU count.
          '';
          example = true;
        };

        workThreads = lib.mkOption {
          type = lib.types.nullOr lib.types.ints.unsigned;
          default = null;
          description = ''
            `workthreads`: Number of audio processing worker threads.
            Only meaningful when `autoWorkerThreads = false`.
          '';
          example = 4;
        };

        workBufferMs = lib.mkOption {
          type = lib.types.nullOr lib.types.ints.unsigned;
          default = null;
          description = ''
            `workbufmsex`: Media buffer size in ms (anticipative FX).
            Default: `1200`.
          '';
          example = 1200;
        };

        workBufferFxUiMs = lib.mkOption {
          type = lib.types.nullOr lib.types.ints.unsigned;
          default = null;
          description = ''
            `workbuffxuims`: Media buffer size in ms when FX UI is open.
            Default: `200`.
          '';
          example = 200;
        };

        preBufferPercent = lib.mkOption {
          type = lib.types.nullOr (lib.types.ints.between 0 100);
          default = null;
          description = ''
            `prebufperb`: Pre-buffer fill percentage (0–100). Default: `100`.
          '';
          example = 100;
        };

        renderAheadLen = lib.mkOption {
          type = lib.types.nullOr lib.types.ints.unsigned;
          default = null;
          description = ''
            `renderaheadlen`: Render-ahead length in ms. Default: `200`.
          '';
          example = 200;
        };

        fxDenorm = lib.mkOption {
          type = lib.types.nullOr lib.types.ints.unsigned;
          default = null;
          description = ''
            `fxdenorm`: Denormalization and plug-in security settings.
            - `&1`  reduce denormalization noise (recommended)
            - `&2`  terminate immediately on exit (skip audio-thread wait)
            Default (factory): `1`.
          '';
          example = 1;
        };

        extra = lib.mkOption {
          type = lib.types.attrsOf lib.types.anything;
          default = { };
          description = "Extra performance key/value pairs for the `[reaper]` section.";
        };
      };

      # ── Category stubs with escape hatches ────────────────────────────────
      # These categories match reaper-config crate modules; full typed options
      # can be added as the crate expands. The `extra` attrset covers any key
      # documented in reaper_config_variables.tsv for that category.

      recording = {
        extra = lib.mkOption {
          type = lib.types.attrsOf lib.types.anything;
          default = { };
          description = ''
            Recording key/value pairs for the `[reaper]` section.
            See `reaper_config_variables.tsv` Recording category for keys.
          '';
          example = {
            recdefbps = 24;
            recdefsrate = 48000;
          };
        };
      };

      appearance = {
        extra = lib.mkOption {
          type = lib.types.attrsOf lib.types.anything;
          default = { };
          description = ''
            Appearance key/value pairs for the `[reaper]` section.
            See `reaper_config_variables.tsv` Appearance category for keys.
          '';
          example = {
            gridinbg = 2;
          };
        };
      };

      midi = {
        extra = lib.mkOption {
          type = lib.types.attrsOf lib.types.anything;
          default = { };
          description = ''
            MIDI key/value pairs for the `[reaper]` section.
            See `reaper_config_variables.tsv` Midi category for keys.
          '';
          example = {
            scoreminnotelen = 4;
          };
        };
      };

      editing = {
        extra = lib.mkOption {
          type = lib.types.attrsOf lib.types.anything;
          default = { };
          description = ''
            Editing key/value pairs for the `[reaper]` section.
            See `reaper_config_variables.tsv` Editing category for keys.
          '';
        };
      };

      automation = {
        extra = lib.mkOption {
          type = lib.types.attrsOf lib.types.anything;
          default = { };
          description = ''
            Automation key/value pairs for the `[reaper]` section.
            See `reaper_config_variables.tsv` Automation category for keys.
          '';
          example = {
            env_autoadd = 1;
          };
        };
      };

      # ── Top-level escape hatches ───────────────────────────────────────────

      extra = lib.mkOption {
        type = lib.types.attrsOf lib.types.anything;
        default = { };
        description = ''
          Arbitrary key/value pairs merged into the `[reaper]` section.
          These take precedence over any typed option in the same section.
        '';
        example = {
          audiodriver = 1;
          undomaxmem = 0;
        };
      };

      extraSections = lib.mkOption {
        type = lib.types.attrsOf (lib.types.attrsOf lib.types.anything);
        default = { };
        description = ''
          Arbitrary additional INI sections appended after `[reaper]`.
          Keys must be the bare section name (without brackets).
        '';
        example = {
          verchk = {
            audiocloseinactive = 0;
            audioclosestop = 0;
          };
        };
      };
    };

    # ── Config text / file outputs ────────────────────────────────────────────

    configText = lib.mkOption {
      type = lib.types.str;
      default = "";
      description = ''
        The `reaper.ini` content as a plain string.

        By default this is computed from `settings.*` (via `lib.mkDefault`).
        You can override it entirely with `lib.mkForce "..."` or a plain
        assignment in the same module set (which beats `lib.mkDefault`).

        To append extra INI sections without replacing the computed output,
        use `settings.extraSections` instead — that's the designed escape hatch.
      '';
      example = ''
        [reaper]
        audiodriver=1
        undomaxmem=0
      '';
    };

    configFile = lib.mkOption {
      type = lib.types.package;
      readOnly = true;
      description = ''
        The `reaper.ini` as a Nix store derivation, always derived from
        `configText`. Use this anywhere a file path is needed:

        - home-manager: `home.file.".config/REAPER/reaper.ini".source = cfg.configFile;`
        - NixOS activation: `system.activationScripts.reaper.text = "install -Dm644 ''${cfg.configFile} /home/alice/.config/REAPER/reaper.ini";`
        - Custom script: pass it as an argument or symlink it.
      '';
    };

    # ── Install script ─────────────────────────────────────────────────────────

    destination = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = ''
        Target path for `installScript`. When set, running `installScript`
        copies `configFile` to this absolute path (creating parent dirs).
        When `null`, `installScript` prints the config to stdout instead.
      '';
      example = "/home/alice/.config/REAPER/reaper.ini";
    };

    installScript = lib.mkOption {
      type = lib.types.package;
      readOnly = true;
      description = ''
        Shell script that writes `configText` to `destination` (if set) or
        prints it to stdout. Useful for imperative or one-shot application:

        ```bash
        nix run .#nixosModules.reaper  # prints to stdout
        # or build and run:
        result/bin/reaper-config-install
        ```

        Wire it into NixOS activation manually if needed:
        ```nix
        system.activationScripts.reaper.text = "''${config.programs.reaper.installScript}/bin/reaper-config-install";
        ```
      '';
    };
  };

  # ── Config implementation ──────────────────────────────────────────────────

  config = lib.mkIf cfg.enable {
    # Computed INI is the default; user assignments at default priority (100)
    # or higher (mkForce) will take precedence.
    programs.reaper.configText = lib.mkDefault iniContent;

    # configFile always tracks whatever configText resolves to.
    programs.reaper.configFile = pkgs.writeText "reaper.ini" cfg.configText;

    # installScript: write to destination path or cat to stdout.
    programs.reaper.installScript =
      let
        destArg = lib.optionalString (cfg.destination != null) cfg.destination;
      in
      pkgs.writeShellScriptBin "reaper-config-install" ''
        set -euo pipefail
        ${
          if cfg.destination != null then
            ''
              install -Dm644 ${cfg.configFile} ${lib.escapeShellArg cfg.destination}
              echo "reaper.ini written to ${cfg.destination}"
            ''
          else
            ''
              cat ${cfg.configFile}
            ''
        }
      '';
  };
}

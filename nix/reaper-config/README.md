# REAPER configuration

The ~350 KB that actually defines this REAPER setup, versioned. The
resource directory it comes from is ~360 MB; almost none of that is
worth keeping.

| here | not here |
|---|---|
| keybindings, toolbars, mouse modifiers | `Scripts/` — ReaPack downloaded them |
| FX tags + folders, screensets | `UserPlugins/` — binaries |
| `reapack.ini` + `ReaPack/registry.db` | `Data/`, `ColorThemes/` — ship with REAPER |
| track/project templates, menu sets | `Reaper/`, `sandbox/` — scratch state |
| the theme in use | `Default_*.ReaperTheme*` — ship with REAPER |
| a **filtered** `reaper.ini` | plugin-scan caches, logs, `.bak`s |

## The ReaPack registry is the manifest

`ReaPack/registry.db` lists the installed packages — 26 of them, from
which ReaPack restores all ~994 scripts. Versioning the downloads
instead would mean vendoring other people's repositories into ours, and
they would go stale the moment upstream moved.

## Our own scripts are an allowlist

`reaper_config::AUTHORED` names the directories that hold scripts we
wrote (`Scripts/FTS`, `Effects/FTS`, …). It is deliberately an
allowlist rather than a blocklist: ReaPack installs into a directory
named after each package's *author*, so a blocklist would have to
enumerate every author on ReaPack and would silently start vendoring
someone else's repository the first time a new package was installed.
An allowlist can only ever be too small — a mistake you notice.

Add a directory there when you start keeping scripts of your own in a
new place.

## `reaper.ini` is filtered, not copied

It mixes real preferences (MIDI editor behaviour, defaults, theme) with
facts about one machine on one day (audio device, window and dock
geometry, recent files). Machine keys are stripped on export, and on
apply the file is **merged** — the target keeps its own hardware and
takes these preferences. Applying config should never reset your sound
card.

## A new machine

```sh
nix run github:FastTrackStudios/FastTrackStudio#fts-reaper
```

REAPER launches already configured: keys, toolbars, mouse modifiers,
theme, templates. Then one ReaPack **Synchronise packages** fetches the
~994 scripts the registry lists, and the machine matches.

### Absolute paths are tokenised

REAPER writes absolute paths for some settings — the active theme among
them (`lastthemefn5=/home/cody/fts-dev/ColorThemes/…`). Those are true
of exactly one machine, so export rewrites the resource-dir prefix to
`$REAPER_RESOURCES` and both `apply` and the `fts-reaper` launcher
expand it again. Without that, a config exported on one machine points
at a directory that does not exist on the next.

### Config is copied, not symlinked

REAPER rewrites these files as you work. A symlink into the read-only
nix store would make every toolbar edit fail, so the launcher copies
them and clears the read-only bit.

## Usage

```sh
fts reaper-config export [resources]   # live → repo
fts reaper-config apply  [resources]   # repo → live (merges reaper.ini)
fts reaper-config diff   [resources]   # what changed
```

Defaults to `$FTS_REAPER_RESOURCES` or `~/fts-dev`. The rules and their
tests live in `crates/daw/daw/src/reaper_config/`.

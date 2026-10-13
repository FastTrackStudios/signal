---
title: Installation & Setup
kind: setup
type: setup
---

# Installation & setup

This is the one-time setup. Everywhere else in the guide assumes the FastTrackStudio REAPER layer is already installed and running — this note is how it got there, and how to confirm it's live if something looks off.

The layer is a native REAPER extension plus a set of styx config files. Install once, and every keybinding, which-key menu, and mode in this guide comes to life over your existing REAPER setup.

## Install the extension

From the repo root:

```sh
just reaper install
```

That builds the release extension, symlinks it into REAPER's `UserPlugins`, and installs the config. Point REAPER at your studio install with `REAPER_HOME` if it isn't the dev default. Restart REAPER and the FTS actions appear in the action list, prefixed `FTS_` / `_FTS_SESSION_`.

## Confirm it's live

Once REAPER restarts, the on-screen status panel tells you which profile and mode are active — if it's showing, the layer loaded:

- `kbd:@FTS_INPUT_TOGGLE_STATUS_PANEL` — Toggle the FTS Input status panel.

```gif
setup-status-panel
The status panel shows the live profile and mode; press a prefix and the which-key overlay teaches the rest.
```

## Where config lives

Profiles, modes, and overlays are plain styx files under `features/reaper/reaper-input/config/config`:

- `fasttrackstudio/*.styx` — the base profile, one file per category (transport, navigation, editing, tracks…).
- `workflows/*.styx` — the modes (Record, Mix, Organize…).
- `overlays/*.styx` — shared keybind overlays modes stack on.

Edit a file, reload, and the binding changes — no rebuild. The same files are embedded in this site, so the guide and the /input reference always match what's on your keyboard.

That's it for setup. Keep the full reference at /input open in another tab as you work, then start with [[Input System|the input layer]] — the mental model the whole profile is built on.

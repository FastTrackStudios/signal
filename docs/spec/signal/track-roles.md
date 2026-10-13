# Track & FX Roles

How REAPER tracks and FX are identified and classified within the Signal system.

## Track Roles

r[signal.roles.track]
Track roles are identified by name prefix, parsed by `TrackRole::parse()`:

r[signal.roles.track.rig]
`[R] Guitar Rig` — A rig-level folder track. Contains engines and FX sends.

r[signal.roles.track.engine]
`[E] Guitar Engine` — An engine-level sub-folder. Contains layers.

r[signal.roles.track.layer]
`[L] Clean Layer` — A layer-level leaf track. Holds the FX chain of
modules and blocks.

## FX Roles

r[signal.roles.fx]
FX roles are identified by name pattern, parsed by `FxRole::parse()`:

r[signal.roles.fx.module]
`{TYPE} Module: {name}` — A module container (e.g., `DRIVE Module: Hype`).
The type prefix maps to `ModuleType` (INPUT, DRIVE, AMP, etc.).

r[signal.roles.fx.block]
`{Type} Block: {name}` — A block (e.g., `Amp Block: JS Amp - Clean`).
The type prefix maps to `BlockType`.

r[signal.roles.fx.prefix]
FX can optionally have `[M]` or `[B]` bracket prefixes which are stripped
before parsing: `[M] DRIVE Module: Hype` → `DRIVE Module: Hype`.

## Display Options

r[signal.roles.display]
`TrackDisplayOptions` controls how roles are formatted:
- `show_prefix`: Show `[R]`/`[E]`/`[L]` brackets
- `show_role`: Show `Rig`/`Engine`/`Layer` keyword
- `show_name`: Show the user-given name
Default: prefix + name (e.g., `[L] Clean Layer`).

## Song Tracks

r[signal.roles.track.song]
`[S] Song Title — Artist` — A song-level folder track used in full-load
setlists. Contains `[L]` variation tracks as children.

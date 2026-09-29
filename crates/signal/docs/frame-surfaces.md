# frame control surfaces

[frame](https://github.com/FastTrackStudios/frame) designs and runs control
surfaces (knobs, faders, footswitches, meters) that are bound to an engine's
parameters. It can be designed by hand, imported from Figma, or designed by
an AI through frame's MCP server. The signal engine serves each rig as a
frame `SurfaceEngine`, so a frame surface drives a live rig the same way the
signal GUIs do: as a vox remote.

## How it's wired

```
signal-desktop --engine  (ws://…/vox)
 ├─ rig services (Rig, RigCore, …)                 unchanged
 └─ SurfaceEngine, scoped per rig ("guitar", …)    crates/signal/surface
      a vox *client* of the rig over the engine's LocalServer
      (like the watch bridge) — catalog / values / set / invoke / select
      + param_changes and telemetry streams
```

`crates/signal/surface` (`signal-surface`) holds one adapter per rig
(`GuitarSurface`, …). Each one:

- builds a frame `Catalog` from the rig's own state. For guitar that is
  `chain()`/`perf()`, with DSP tapers and units from
  `signal_sampler::native::range_of`.
- converts frame's normalized values to the rig's real values (the rig's
  `set_block_param` takes real values).
- mirrors the rig's `#[subscribe]` events into frame's `param_changes` and
  `telemetry`.
- marks `featured` params (block primary, bypass, type, master) so generated
  and AI-designed surfaces start from what a performer reaches for.

It is mounted in `apps/desktop/src/engine_main.rs` with
`merge_router_scoped(Rig::Guitar.slug(), …)`, next to `RigCore`.

## Guitar addresses

| address | kind | rig call |
|---|---|---|
| `<block>/<param>` | continuous | `set_block_param(block, param, real)` |
| `<block>/on` | toggle | `set_block_bypass(block, !on)` |
| `<block>/type` | choice | `set_block_option(block, i)` |
| `rig/master_trim` | dB | `set_master_trim(db)` |

Block ids are the rig's (display names, de-duplicated: `DLY 2`).

- **Actions:** `tap_tempo`, `tuner`, `fx`, `boost`, `cycle_boost`, `mute`,
  `next_song`, `prev_song`, and `stack/<i>` for each stack.
- **Streams:** `in_level` and `out_level` (dBFS, L/R), `gain_reduction`,
  `spectrum` (96 bins), `tempo`.
- **Collections:** `songs`, `setlists`, `parts`, `patches`.

## Using it

```sh
# the engine (any port; 4040 is the default)
cargo build -p signal-desktop && ./target/debug/signal-desktop --engine

# from the frame repo:
engine_probe --url ws://127.0.0.1:4040/vox --rig guitar            # catalog + values
engine_probe --url ws://127.0.0.1:4040/vox --set "DLY 1/mix=0.35"   # round trip
engine_probe --url ws://127.0.0.1:4040/vox --catalog-out guitar.json
render_surface --catalog guitar.json --out guitar.png               # generated surface
```

## Versions

frame pins the same architect (`v0.0.2`), facet, vox and vello as this
repo. signal consumes `surface-proto` as a git dependency pinned to a frame
tag. When bumping the architect stack here, bump frame alongside it.

## Not done yet

- A `SurfaceEngine` for every rig. The other rigs follow the guitar pattern.
- Embedding frame surfaces *inside* the Blitz GUI. The pinned dioxus-native
  doesn't expose vello's `register_custom_paint_source`, which a `<canvas>`
  would need. Until it does, frame's own `frame` player runs as a remote.

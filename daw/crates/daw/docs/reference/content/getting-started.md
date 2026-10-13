+++
title = "Getting Started"
weight = 1
+++

Building and running the DAW extension for REAPER.

## Build

```bash
cargo build
cargo check -p daw
```

## Test

Use `cargo xtask test` for the default local test pass. This delegates to
`cargo nextest run --profile default` and uses `.config/nextest.toml`, matching
the lightweight nextest setup used in Bearcove Rust repositories.

Real REAPER integration tests are opt-in because they spawn REAPER and require a
configured test rig:

```bash
cargo xtask test --reaper
cargo xtask test --reaper actions_cli
```

`cargo xtask reaper-test` remains available as the direct REAPER harness entry
point.

## Run Against REAPER

Install or load the `daw-bridge` REAPER extension, then connect external tools
to the Unix socket it serves. By default the socket is
`/tmp/fts-daw-{pid}.sock`; set `FTS_SOCKET` before launching REAPER to force a
specific path.

```bash
daw info
daw tracks --json
daw transport
```

The CLI auto-discovers live `/tmp/fts-daw-*.sock` files when `--socket` is not
provided.

## Integrated Extensions

Code already running inside REAPER should prefer local service access instead
of the Unix socket. Use the `daw` facade and the in-process helpers exposed by
`daw-control-sync` / `daw-reaper` when building integrated extensions or audio
plugins.

## Screensets

Named FTS screensets are available through the service layer and CLI:

```bash
daw screensets
daw screenset-capture live --name "Live" --kind window
daw screenset-apply live
```

Screensets are separate from REAPER's numbered slots and are intended to become
the universal FTS workspace model for window layouts, track visibility sets, and
selection/time-range sets.

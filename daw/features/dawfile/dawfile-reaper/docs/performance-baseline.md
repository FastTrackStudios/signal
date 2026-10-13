# dawfile-reaper Performance Baseline

## Fixtures
- Tempo-heavy fixture (tracked): `/Users/codywright/Documents/Development/Rust/roam-test/modules/dawfile/dawfile-reaper/tests/fixtures/tempo-map-advanced.RPP`
- Large real-world fixture (local, git-ignored): `/Users/codywright/Documents/Development/Rust/roam-test/modules/dawfile/dawfile-reaper/tests/fixtures/local/Goodness of God.RPP`

## Canonical Command Set
### Perf matrix script (recommended)
```bash
modules/dawfile/dawfile-reaper/scripts/perf_matrix.sh
```

This runs `rpp_perf` in release mode with the canonical fixture pair:
- `tests/fixtures/tempo-map-advanced.RPP` (tempo-heavy)
- `tests/fixtures/local/Goodness of God.RPP` (large real-world)

Artifacts:
- `modules/dawfile/dawfile-reaper/docs/perf-runs/latest.md` (summary table)
- `modules/dawfile/dawfile-reaper/docs/perf-runs/latest.txt` (raw CLI output)

### Direct CLI timing utility (equivalent)
```bash
cargo run -p dawfile-reaper --release --bin rpp_perf -- \
  --fixture modules/dawfile/dawfile-reaper/tests/fixtures/tempo-map-advanced.RPP \
  --fixture "modules/dawfile/dawfile-reaper/tests/fixtures/local/Goodness of God.RPP" \
  --warmup 1 \
  --repeat 3 \
  --typed-mode full
```

### Criterion benchmark
```bash
cargo bench -p dawfile-reaper --bench parse_perf
```

Optional:
```bash
RPP_LARGE_FIXTURE="/absolute/path/to/large.RPP" \
  cargo bench -p dawfile-reaper --bench parse_perf
```

## Baseline (2026-02-10)
From `/Users/codywright/Documents/Development/Rust/roam-test/modules/dawfile/dawfile-reaper/tests/full_project_audit.rs` against `Goodness of God.RPP`:
- fixture size: `418.82 MB`
- `parse_rpp_file`: `40.276s` (~`10.40 MB/s`)
- typed conversion: `0.527s`

## Current Baseline (2026-02-11)
From `rpp_perf` (`--release --warmup 1 --repeat 3 --typed-mode full`):

### tempo-map-advanced.RPP
- fixture size: `0.01 MB`
- parse avg: `0.0001s` (~`59.79 MB/s`)
- typed avg: `0.0000s`
- peak RSS: `2.19 MB`
- parse alloc calls avg: `473`
- parse alloc MB avg: `0.06`
- typed alloc calls avg: `262`
- typed alloc MB avg: `0.04`

### Goodness of God.RPP
- fixture size: `418.82 MB`
- parse avg: `0.3174s` (~`1319.62 MB/s`)
- typed avg: `0.0549s`
- peak RSS: `1295.77 MB`
- parse alloc calls avg: `4,362,878`
- parse alloc MB avg: `752.15`
- typed alloc calls avg: `529,959`
- typed alloc MB avg: `70.40`

Notes:
- Allocation telemetry mode adds overhead and should be used for relative allocation comparisons between commits, not absolute throughput comparisons.

## Hotspots Addressed
- Replaced recursive/nom-first project parsing with fast streaming stack parser (with compatibility fallback).
- Added fast token classification path to avoid generic parser overhead on common unquoted lines.
- Added specialized block-header parsing for common `<NAME ...>` headers to avoid full token parser in hot path.
- Added parallel typed decode for independent domains with deterministic merge.
- Reduced temporary allocations in fast tokenization (removed per-line `Vec<&str>` split collection and unconditional float normalization allocation).

## Notes
- `rpp_perf` reports `peak_rss_mb` via `getrusage(RUSAGE_SELF)`.
- macOS reports bytes directly, Linux reports KiB and is converted to bytes.
- Keep hardware and build profile consistent when comparing runs.

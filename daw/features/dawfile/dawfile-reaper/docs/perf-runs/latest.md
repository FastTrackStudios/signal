# Performance Matrix (20260211-110007)

- command:
  ```bash
  cargo run -p dawfile-reaper --release --bin rpp_perf -- --fixture "/Users/codywright/Documents/Development/Rust/roam-test/modules/dawfile/dawfile-reaper/tests/fixtures/tempo-map-advanced.RPP" --fixture "/Users/codywright/Documents/Development/Rust/roam-test/modules/dawfile/dawfile-reaper/tests/fixtures/local/Goodness of God.RPP" --warmup "1" --repeat "3" --typed-mode "full"
  ```
- raw output: `/Users/codywright/Documents/Development/Rust/roam-test/modules/dawfile/dawfile-reaper/docs/perf-runs/perf-matrix-20260211-110007.txt`

| Fixture | Size MB | Parse Avg s | Typed Avg s | Throughput MB/s | Peak RSS MB | Parse Alloc Calls | Parse Alloc MB | Typed Alloc Calls | Typed Alloc MB |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `/Users/codywright/Documents/Development/Rust/roam-test/modules/dawfile/dawfile-reaper/tests/fixtures/tempo-map-advanced.RPP` | 0.01 | 0.0001 | 0.0000 | 59.79 | 2.19 | 473 | 0.06 | 262 | 0.04 |
| `/Users/codywright/Documents/Development/Rust/roam-test/modules/dawfile/dawfile-reaper/tests/fixtures/local/Goodness of God.RPP` | 418.82 | 0.3174 | 0.0549 | 1319.62 | 1295.77 | 4362878 | 752.15 | 529959 | 70.40 |

//! Round-trip every bundled fixture.
//!
//! `cargo test --workspace` must exercise the round-trip check on every
//! fixture file, not just the one a developer happened to try.
//!
//! This used to shell out to `cargo build -p dawfile-protools --example
//! round_trip` and then spawn that binary once per fixture. Building the
//! example took 38 seconds and the round-tripping takes 0.3, so the test
//! spent its whole life being killed by nextest's 30-second slow-timeout —
//! it has never passed in CI. The check now lives in the library
//! (`dawfile_protools::check_round_trip`) and runs in-process.

use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn round_trip_every_fixture() {
    let entries = std::fs::read_dir(fixtures_dir()).expect("read fixtures dir");

    let mut count = 0usize;
    let mut failures = Vec::new();
    for entry in entries {
        let path = entry.expect("read fixtures dir entry").path();
        let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
            continue;
        };
        if !matches!(ext, "ptx" | "ptf" | "pts") {
            continue;
        }
        count += 1;
        if let Err(e) = dawfile_protools::check_round_trip(&path) {
            failures.push(format!("{}:\n  {e}", path.display()));
        }
    }

    assert!(count > 0, "no fixtures discovered");
    assert!(
        failures.is_empty(),
        "{} of {count} fixtures failed round-trip:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

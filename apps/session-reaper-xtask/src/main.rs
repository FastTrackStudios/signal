//! Runs the session repo's `#[reaper_test]` suites against a real REAPER.
//!
//! The daw repo's runner used to list `session` among its packages and
//! stopped when the repos split — its comment says these tests "run from
//! that repo's own runner", which until now did not exist. This is it.
//!
//! What it stands up is an isolated rig under `target/fts-reaper-test`,
//! with ONLY the host extension in its UserPlugins, so the dev rig's
//! SWS / ReaPack / fts-extensions installs cannot change what a test
//! sees. The host is `daw-bridge`, built from the sibling `../daw`
//! checkout — the same sibling everything else here compiles against.
//!
//! The panel finds that REAPER the way it finds any REAPER: by
//! discovering the socket the host publishes. That is the point. A test
//! that reached in through a handle the harness already had would prove
//! the services work and say nothing about whether the WINDOW can
//! attach to a DAW it did not start.
//!
//! Usage:
//!   cargo run -p session-reaper-xtask              # every suite
//!   cargo run -p session-reaper-xtask `<filter>`     # matching test NAMES
//!   cargo run -p session-reaper-xtask -- --gui     # watch it drive REAPER

use daw::test::runner::{TestPackage, TestRunner};
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let filter = args.iter().skip(1).find(|a| !a.starts_with("--")).cloned();
    let gui = args.iter().any(|a| a == "--gui");
    let virtual_display = args.iter().any(|a| a == "--virtual");
    let keep_open = args.iter().any(|a| a == "--keep-open");

    // This crate sits at apps/session-reaper-xtask; the repo root is two
    // levels up.
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .map_err(|e| format!("session repo root not found: {e}"))?;
    let daw_root = repo_root
        .join("../daw")
        .canonicalize()
        .map_err(|e| format!("sibling ../daw checkout not found: {e}"))?;

    println!("=== session REAPER integration tests ===");
    println!("  Workspace: {}", repo_root.display());
    println!("  Host from: {}", daw_root.display());

    let resources_dir = prepare_rig(&repo_root)?;
    println!("  Rig:       {}", resources_dir.display());
    // Child test processes resolve the rig through these.
    unsafe {
        std::env::set_var("FTS_REAPER_CONFIG", &resources_dir);
        std::env::set_var("FTS_REAPER_RESOURCES", &resources_dir);
    }

    // These tests talk to a REAPER that is already up: a few RPCs and
    // a poll tick each, seconds for the lot. The timeout is not a
    // budget, it is what a HUNG run costs before it admits it. Raise it
    // by name when something genuinely needs longer:
    // `REAPER_TEST_TIMEOUT_SECS=600 just reaper-test`.
    let timeout_secs: u64 = std::env::var("REAPER_TEST_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(90);
    let mut runner = TestRunner::new(&resources_dir).with_timeout(timeout_secs);
    if gui {
        runner = runner.with_headless(false);
        println!("  Mode:      GUI (visible REAPER window)");
    }
    if virtual_display {
        runner = runner.with_virtual_display()?;
        println!("  Mode:      virtual display (dialogs get dismissed)");
    }
    if keep_open {
        runner.keep_open = true;
        println!("  Keep open: REAPER stays up after the run");
    }

    let packages = vec![TestPackage {
        package: "session-daw".into(),
        // The suites are behind this feature so an ordinary
        // `cargo nextest run --workspace` does not try to talk to a
        // REAPER that is not there.
        features: vec!["reaper-tests".into()],
        // One at a time: they share one REAPER, and two tests renaming
        // tracks in the same instance is a race dressed as a flake.
        test_threads: 1,
        default_skips: vec![],
        test_binary: None,
    }];

    runner.install_daw_bridge(&daw_root)?;
    runner.build_test_packages(&repo_root, &packages)?;
    let passed = runner.run_reaper_tests(&packages, filter.as_deref())?;

    if passed {
        println!("\n  All tests passed!");
        Ok(())
    } else {
        Err("Some tests failed".into())
    }
}

/// Build the isolated rig, reusing the dev rig's read-only data.
///
/// Only the host extension goes in UserPlugins. Themes, cursors and the
/// rest are symlinked from the dev install when it exists, because they
/// are large, read-only, and identical.
fn prepare_rig(repo_root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let rig = repo_root.join("target").join("fts-reaper-test");
    std::fs::create_dir_all(rig.join("UserPlugins"))?;
    let source = daw::test::runner::fts_reaper_resources();
    for name in [
        "ColorThemes",
        "Cursors",
        "Data",
        "Effects",
        "FXChains",
        "LangPack",
        "MIDINoteNames",
        "Scripts",
        "TrackTemplates",
    ] {
        let from = source.join(name);
        let to = rig.join(name);
        if from.exists() && !to.exists() {
            let _ = std::os::unix::fs::symlink(&from, &to);
        }
    }
    Ok(rig)
}

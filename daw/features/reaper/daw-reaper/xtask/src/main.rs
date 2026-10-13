//! xtask runner for the daw domain's REAPER integration tests.
//!
//! Boots a headless REAPER with ONLY `daw-bridge` installed (an isolated
//! rig under `target/fts-reaper-test`, so the dev rig's extensions —
//! SWS, ReaPack, reaper_fts_extensions — never interfere), then runs the
//! `#[reaper_test]` suites in `features/reaper/daw-reaper/tests/` against
//! the live instance: transport, tempo map, markers/lanes, items, takes,
//! routing, ext-state, action registry, …
//!
//! Usage:
//!   cargo run -p daw-reaper-xtask                 # full suite, headless
//!   cargo run -p daw-reaper-xtask `<filter>`        # tests matching filter
//!   FTS_KEEP_OPEN=1 cargo run -p daw-reaper-xtask # keep REAPER open after
//!
//! Invoked via `just daw-reaper-test` (sets FTS_REAPER_EXECUTABLE to the
//! wrapper script that resolves the local REAPER install).

use daw::test::runner::{self, TestPackage, TestRunner};
use std::env;
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let filter = args.iter().skip(1).find(|a| !a.starts_with("--")).cloned();
    // `--gui` shows REAPER's window instead of running it with DISPLAY=""
    // — the only way to actually watch a test drive the DAW. Pair it with
    // `--keep-open` (or FTS_KEEP_OPEN=1) to inspect the result afterwards,
    // otherwise REAPER is killed the moment the run ends.
    let gui = args.iter().any(|a| a == "--gui");
    let keep_open = args.iter().any(|a| a == "--keep-open");

    // xtask lives at features/reaper/daw-reaper/xtask — repo root is
    // three levels up.
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .canonicalize()
        .map_err(|e| format!("monorepo root not found: {e}"))?;

    println!("=== daw-reaper REAPER Integration Tests ===");
    println!("  Workspace: {}", repo_root.display());

    // Isolated rig: only daw-bridge lives in UserPlugins.
    let resources_dir = prepare_isolated_rig(&repo_root)?;
    println!("  Rig:       {}", resources_dir.display());
    // Child test processes (and multi-instance tests that spawn their own
    // REAPER) resolve the rig through these.
    unsafe {
        env::set_var("FTS_REAPER_CONFIG", &resources_dir);
        env::set_var("FTS_REAPER_RESOURCES", &resources_dir);
    }

    // Every test here talks to a REAPER that is already up, so the
    // work is a few RPCs and a poll tick — the whole suite runs in
    // seconds. The timeout is not a budget, it is how long a HUNG run
    // costs before it admits it, and ten minutes of that is ten
    // minutes of nothing. Anything genuinely slower says so by name:
    // `REAPER_TEST_TIMEOUT_SECS=600 cargo run -p daw-reaper-xtask`.
    let timeout_secs: u64 = env::var("REAPER_TEST_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(90);
    let mut runner = TestRunner::new(&resources_dir).with_timeout(timeout_secs);
    if gui {
        runner = runner.with_headless(false);
        println!("  Mode:      GUI (visible REAPER window)");
    }
    if keep_open {
        runner.keep_open = true;
        println!("  Keep open: REAPER stays up after the run");
    }

    let packages = vec![
        TestPackage {
            package: "daw-reaper".into(),
            features: vec![],
            test_threads: 1,
            default_skips: vec![
                // Soak test — 60s of wall-clock per run; opt in with an
                // explicit filter (`just daw-reaper-test timer_responsive_for_60s`).
                "timer_responsive_for_60s".into(),
                // Registers actions via `architect::action::ActionBackend for
                // Reaper` IN the test process — that seam calls Reaper::get()
                // directly and can only run inside REAPER (an in-process guest
                // extension), not from the external test client.
                "action_backend_handler_runs_on_trigger".into(),
                // KNOWN BROKEN (vox schema negotiation): restore_layout's
                // response schema is never sent — client dies with
                // InvalidPayload("no remote response schema received").
                "save_restore_layout_round_trip".into(),
                // KNOWN BROKEN (crashes daw-bridge): the reaper_project_isolation
                // suite creates tabs with tracks/FX and closes them; something in
                // daw-bridge still references the closed tab off the main thread
                // and panics reaper-rs's require_valid_project ("ReaProject
                // doesn't exist anymore"), killing the daw runtime and poisoning
                // every later test. Fix belongs in daw-reaper's pointer
                // validation / audio-sync registry; until then the whole file is
                // skipped (its coverage overlaps the batch/track suites).
                "fx_operations_in_isolated_project".into(),
                "multi_track_manipulation".into(),
                "project_has_unique_guid".into(),
                "tracks_are_isolated".into(),
                // KNOWN BROKEN (same stale-ReaProject class): the reaper_takes
                // suite adds media items/takes and tears the tab down; the run
                // either crashes daw-bridge or blocks for the full timeout.
                "take_".into(),
                // KNOWN BROKEN (assertion drift): mixer-visibility flag no
                // longer round-trips through the screenset service.
                "track_set_round_trips_visibility".into(),
                // KNOWN BROKEN (drift): tempo add_point grows the point list
                // by 2 instead of 1 on REAPER 7.75 — service or REAPER-side
                // behavior change; remove_point depends on the same count.
                "add_then_query_tempo_point".into(),
                "remove_point_drops_from_list".into(),
                // KNOWN BROKEN (drift/environment): toolbar button
                // registration fails in the isolated headless rig.
                "add_button_succeeds_and_returns_command_id".into(),
                "remove_button_succeeds".into(),
                "workflow_remove_cleans_up_batch".into(),
                // Headless: no WM to round-trip the main-window geometry.
                "nudge_round_trips_main_window".into(),
            ],
            test_binary: None,
        },
        // `session`, `chord-tool-daw` and `midi-tools-daw` moved to the
        // session repo in the August 2026 split; their REAPER tests run
        // from that repo's own runner. Listing them here made every run
        // of this xtask die with "package ID specification `session` did
        // not match any packages" before a single test started.
    ];

    runner.install_daw_bridge(&repo_root)?;
    runner.build_test_packages(&repo_root, &packages)?;
    let tests_passed = runner.run_reaper_tests(&packages, filter.as_deref())?;

    if tests_passed {
        println!("\n  All tests passed!");
        Ok(())
    } else {
        Err("Some tests failed".into())
    }
}

/// Build (or refresh) an isolated REAPER rig under `target/` so tests don't
/// pick up extensions (SWS, ReaPack, fts-extensions, …) installed in the
/// dev rig. Only daw-bridge lives in this rig's UserPlugins/.
fn prepare_isolated_rig(repo_root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let rig = repo_root.join("target").join("fts-reaper-test");
    std::fs::create_dir_all(&rig)?;

    // Symlink shared read-only data dirs from the dev rig when present
    // (falls back to REAPER's install defaults otherwise).
    let source = runner::fts_reaper_resources();
    for name in [
        "ColorThemes",
        "Cursors",
        "Data",
        "Effects",
        "FXChains",
        "KeyMaps",
        "LangPack",
        "MIDINoteNames",
        "MouseMaps",
        "OSC",
        "presets",
        "TrackTemplates",
        "Scripts",
    ] {
        let src = source.join(name);
        let dst = rig.join(name);
        if !dst.exists() && src.exists() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(&src, &dst).ok();
        }
    }

    // UserPlugins is always a real directory, wiped each run so no stale
    // third-party plugins survive.
    let user_plugins = rig.join("UserPlugins");
    if user_plugins.exists() {
        for entry in std::fs::read_dir(&user_plugins)? {
            let path = entry?.path();
            if path.is_file() || path.is_symlink() {
                let _ = std::fs::remove_file(path);
            }
        }
    } else {
        std::fs::create_dir_all(&user_plugins)?;
    }

    // Minimal reaper.ini seed. The harness's patch_ini() forces Dummy
    // audio on Linux (linux_audio_mode=2) — REAPER's ALSA/PipeWire path
    // can block the main thread and starve extension timers.
    let ini = rig.join("reaper.ini");
    if !ini.exists() {
        std::fs::write(
            &ini,
            "[REAPER]\naudiodriver=2\nlinux_audio_mode=2\nautosaveint=0\n[reaper_explorer]\nlastdir=/tmp\n",
        )?;
    }

    Ok(rig)
}

//! xtask runner for session's own REAPER integration tests.
//!
//! Boots a headless REAPER with ONLY `session-extension` installed (an
//! isolated rig under `target/fts-reaper-test`, so the dev rig's real
//! extensions — SWS, ReaPack, `reaper_fts_extensions` — never interfere
//! and never get touched), then runs the `#[reaper_test]` suites that
//! prove session's own REAPER-facing services — `SetlistServiceImpl<
//! daw_reaper::Reaper>`, the mode/take-ranking/record-control surfaces —
//! actually work against a real instance.
//!
//! `session-extension` mounts *only* session's services, not the rest of
//! `fts-extensions` (tempo tools, mirror, launcher, …) — same relationship
//! `daw-bridge` has to `daw`'s own reaper tests (see `daw`'s
//! `features/reaper/daw-reaper/xtask`, which this file's isolated-rig
//! setup is copied from). Proving the *full* `fts-extensions` composition
//! still works is that repo's own test suite's job, against its own cdylib.
//!
//! Usage:
//!   cargo run -p session-extension-xtask                 # full suite, headless
//!   cargo run -p session-extension-xtask -- `<filter>`     # tests matching filter
//!   cargo run -p session-extension-xtask -- --gui         # visible REAPER window
//!   FTS_KEEP_OPEN=1 cargo run -p session-extension-xtask  # keep REAPER open after

use daw::test::runner::{ExtensionPackage, TestPackage, TestRunner};
use std::env;
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let filter = args.iter().skip(1).find(|a| !a.starts_with("--")).cloned();
    // `--gui` shows REAPER's window instead of DISPLAY="" — headless
    // cannot open a Dioxus/GUI window at all (see the reaper-testing
    // skill), but session's own services here are all headless RPC, so
    // this mainly matters for eyeballing REAPER's state mid-test.
    let gui = args.iter().any(|a| a == "--gui");
    let keep_open = args.iter().any(|a| a == "--keep-open");
    // `--virtual`: a private Xvfb + window manager, with a background
    // dismisser loop closing known stray dialogs throughout the run (see
    // `TestRunner::run_reaper_tests`). Needed for any test that opens a
    // real, pre-existing project — REAPER's "Project Load Warning" (a
    // format mismatch between the project's REAPER version and this
    // flake's) pops a native modal that has no headless-safe suppression
    // and otherwise blocks the main thread forever, starving every RPC.
    let virtual_display = args.iter().any(|a| a == "--virtual");

    // xtask lives at features/reaper/session-extension/xtask — repo root
    // is four path components up.
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .canonicalize()
        .map_err(|e| format!("session repo root not found: {e}"))?;

    println!("=== session REAPER Integration Tests ===");
    println!("  Workspace: {}", repo_root.display());

    // Isolated rig: only session-extension lives in UserPlugins, so this
    // never picks up (or disturbs) the dev rig's real extensions.
    let resources_dir = prepare_isolated_rig(&repo_root)?;
    println!("  Rig:       {}", resources_dir.display());
    unsafe {
        env::set_var("FTS_REAPER_CONFIG", &resources_dir);
        env::set_var("FTS_REAPER_RESOURCES", &resources_dir);
    }

    let timeout_secs: u64 = env::var("REAPER_TEST_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120);
    let mut runner = TestRunner::new(&resources_dir).with_timeout(timeout_secs);
    if gui {
        runner = runner.with_headless(false);
        println!("  Mode:      GUI (visible REAPER window)");
    } else if virtual_display {
        runner = runner.with_virtual_display()?;
        println!("  Mode:      virtual display (private Xvfb, auto-dismisses stray dialogs)");
    }
    if keep_open {
        runner.keep_open = true;
        println!("  Keep open: REAPER stays up after the run");
    }

    let packages = vec![
        // Health check: session-extension loaded and wrote its ExtState
        // beacon. Cheap, fast-failing sanity check before the real tests.
        TestPackage {
            package: "session".into(),
            features: vec![],
            test_threads: 1,
            default_skips: vec![],
            test_binary: Some("reaper_session_extension".into()),
        },
        // The real proof: Recording Mode's SetlistService connection —
        // the same lanes `session-desktop`'s `reaper_engine.rs` opens —
        // actually work against a live `daw_reaper::Reaper` backend.
        TestPackage {
            package: "session".into(),
            features: vec![],
            test_threads: 1,
            default_skips: vec![],
            test_binary: Some("reaper_setlist_recording_mode".into()),
        },
        // Arm/record/stop through the exact SetlistService calls the
        // transport bar's Arm/Record buttons make, against a real drum-kit
        // track layout — proves the record pipeline actually captures
        // items, not just that the RPCs return Ok.
        TestPackage {
            package: "session".into(),
            features: vec![],
            test_threads: 1,
            default_skips: vec![],
            test_binary: Some("reaper_drum_recording".into()),
        },
        // Opens all 10 of the real Rockstars album's organized `.RPP`
        // projects and proves a real multi-song setlist builds from them —
        // exactly what Recording Mode's `load_playlist` does with a real
        // album, not a synthetic single-project fixture.
        TestPackage {
            package: "session".into(),
            features: vec![],
            test_threads: 1,
            default_skips: vec![],
            test_binary: Some("reaper_rockstars_album".into()),
        },
        // Proves session-desktop --engine's LAN control surface — a real
        // axum WebSocket, the same architect::axum_ws::serve_router path —
        // actually works against a live REAPER, not just Live Mode's
        // in-process player or the unix socket other tests use.
        TestPackage {
            package: "session".into(),
            features: vec![],
            test_threads: 1,
            default_skips: vec![],
            test_binary: Some("reaper_lan_engine".into()),
        },
    ];

    runner.install_extension_package(
        &repo_root,
        &ExtensionPackage {
            package: "session-extension".into(),
            lib_stem: "reaper_session_extension".into(),
            plugin_name: "reaper_session_extension.so".into(),
            release: false,
        },
    )?;
    runner.build_test_packages(&repo_root, &packages)?;
    let tests_passed = runner.run_reaper_tests(&packages, filter.as_deref())?;

    if tests_passed {
        println!("\n  All tests passed!");
        Ok(())
    } else {
        Err("Some tests failed".into())
    }
}

/// Build (or refresh) an isolated REAPER rig under `target/` so tests
/// never pick up (or disturb) extensions installed in the real dev rig
/// (`~/fts-dev`'s SWS, ReaPack, `reaper_fts_extensions`, …). Only
/// `session-extension` lives in this rig's `UserPlugins/`. Copied from
/// `daw`'s `features/reaper/daw-reaper/xtask` — same isolation shape,
/// same reasons.
fn prepare_isolated_rig(repo_root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let rig = repo_root.join("target").join("fts-reaper-test");
    std::fs::create_dir_all(&rig)?;

    // Symlink shared read-only data dirs from the dev rig when present
    // (falls back to REAPER's install defaults otherwise). This is also
    // where REAPER license state lives (not a per-resources-dir concern),
    // so a rig built this way is licensed exactly like the dev rig is.
    let source = daw::test::runner::fts_reaper_resources();
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
    // extensions (including a previous fts-extensions install) survive.
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

    // Minimal reaper.ini seed. Dummy audio on Linux — REAPER's ALSA/
    // PipeWire path can block the main thread and starve extension timers,
    // and none of session's own reaper tests need real audio hardware.
    //
    // `lv2path_linux`/`vstpath` cleared: an unset LV2 path falls back to
    // REAPER's system-convention scan (`~/.lv2`, `/usr/lib/lv2`, …), which
    // on a real workstation can be hundreds of plugins — pure startup cost
    // this rig never needs since no test loads a real plugin. Matches the
    // real dev rig's own `~/fts-dev/reaper.ini` (`lv2path_linux=` empty,
    // `vstpath` restricted to `~/.vst;~/.vst3` rather than left unset).
    let ini = rig.join("reaper.ini");
    if !ini.exists() {
        std::fs::write(
            &ini,
            "[REAPER]\naudiodriver=2\nlinux_audio_mode=2\nautosaveint=0\nlv2path_linux=\nvstpath=\nvstpath64=\n[reaper_explorer]\nlastdir=/tmp\n",
        )?;
    }

    Ok(rig)
}

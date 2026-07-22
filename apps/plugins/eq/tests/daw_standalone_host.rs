//! Hosted CLAP smoke tests for FTS-EQ.
//!
//! These tests deliberately drive the local DAW workspace as an external
//! process instead of linking `daw-standalone` into this crate. The two
//! workspaces currently pin some shared dependencies differently, and compiling
//! the host in its own workspace is closer to how a real plugin host loads the
//! finished CLAP bundle anyway.
//!
//! Build/package the plugin first, then run:
//!
//! ```bash
//! DAW_TEST_FTS_EQ_CLAP_BUNDLE=/path/to/eq-plugin.clap \
//! FTS_EQ_RUN_DAW_STANDALONE_TESTS=1 \
//! cargo test -p eq-plugin --test daw_standalone_host -- --nocapture
//! ```
//!
//! For the native GUI window smoke test:
//!
//! ```bash
//! DAW_TEST_FTS_EQ_CLAP_BUNDLE=/path/to/eq-plugin.clap \
//! FTS_EQ_RUN_DAW_STANDALONE_TESTS=1 \
//! DAW_TEST_CLAP_GUI_HOLD_SECS=5 \
//! cargo test -p eq-plugin --test daw_standalone_host \
//!   hosted_fts_eq_gui_opens -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn daw_workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../daw")
        .canonicalize()
        .expect("local ../../daw workspace should exist")
}

fn fts_eq_bundle_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("DAW_TEST_FTS_EQ_CLAP_BUNDLE") {
        return Some(PathBuf::from(path));
    }

    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root should exist");

    [
        repo.join("target/bundled/eq-plugin.clap"),
        repo.join("target/debug/eq-plugin.clap"),
        repo.join("target/release/eq-plugin.clap"),
        PathBuf::from("/home/cody/.clap/eq-plugin.clap"),
        PathBuf::from("/home/cody/fasttrackstudio/UserPlugins/FX/eq-plugin.clap"),
    ]
    .into_iter()
    .find(|p| p.exists())
}

fn run_enabled() -> bool {
    std::env::var_os("FTS_EQ_RUN_DAW_STANDALONE_TESTS").is_some()
}

fn run_daw_standalone_test(test_name: &str, bundle_path: &Path, ignored: bool) -> Output {
    let mut command = Command::new("cargo");
    command
        .current_dir(daw_workspace())
        .env("DAW_TEST_FTS_EQ_CLAP_BUNDLE", bundle_path)
        .arg("test")
        .arg("-p")
        .arg("daw-standalone")
        .arg("--features")
        .arg("clap-host")
        .arg(test_name)
        .arg("--")
        .arg("--nocapture");

    if ignored {
        command.arg("--ignored");
    }

    command
        .output()
        .expect("should launch cargo test for daw-standalone")
}

fn assert_daw_test_passed(output: Output, test_name: &str) {
    if output.status.success() {
        return;
    }

    panic!(
        "daw-standalone test {test_name} failed with status {:?}\n\nstdout:\n{}\n\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn hosted_fts_eq_descriptor_and_audio_smoke_pass() {
    if !run_enabled() {
        eprintln!("(skip) set FTS_EQ_RUN_DAW_STANDALONE_TESTS=1 to run the local DAW host smoke");
        return;
    }
    let Some(path) = fts_eq_bundle_path() else {
        eprintln!("(skip) set DAW_TEST_FTS_EQ_CLAP_BUNDLE to the FTS-EQ .clap bundle path");
        return;
    };

    assert_daw_test_passed(
        run_daw_standalone_test("lists_descriptors_in_a_real_bundle", &path, false),
        "lists_descriptors_in_a_real_bundle",
    );
    assert_daw_test_passed(
        run_daw_standalone_test("processes_audio_through_a_real_plugin", &path, false),
        "processes_audio_through_a_real_plugin",
    );
}

#[test]
#[ignore = "manual/CI GUI test: opens the FTS-EQ CLAP plugin window through daw-standalone"]
fn hosted_fts_eq_gui_opens() {
    if !run_enabled() {
        eprintln!(
            "(skip) set FTS_EQ_RUN_DAW_STANDALONE_TESTS=1 to run the local DAW host GUI smoke"
        );
        return;
    }
    let Some(path) = fts_eq_bundle_path() else {
        eprintln!("(skip) set DAW_TEST_FTS_EQ_CLAP_BUNDLE to the FTS-EQ .clap bundle path");
        return;
    };

    assert_daw_test_passed(
        run_daw_standalone_test("clap_fts_eq_gui_opens", &path, true),
        "clap_fts_eq_gui_opens",
    );
}

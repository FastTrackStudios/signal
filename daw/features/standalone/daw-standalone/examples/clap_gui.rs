//! Manual CLAP GUI launcher.
//!
//! Run from a graphical session:
//!
//! ```bash
//! cargo run -p daw-standalone --features clap-host --example clap_gui -- \
//!   /path/to/lsp-plugins.clap
//! ```

#[cfg(feature = "clap-host")]
use std::path::PathBuf;
#[cfg(feature = "clap-host")]
use std::time::Duration;

#[cfg(feature = "clap-host")]
use daw_standalone::audio_engine::plugin_host::{ClapHost, ClapPluginSelector};

#[cfg(feature = "clap-host")]
fn main() {
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("DAW_TEST_CLAP_GUI_BUNDLE").map(PathBuf::from))
        .or_else(|| std::env::var_os("DAW_TEST_FTS_EQ_CLAP_BUNDLE").map(PathBuf::from))
        .or_else(|| std::env::var_os("DAW_TEST_LSP_CLAP_BUNDLE").map(PathBuf::from))
        .expect("usage: clap_gui /path/to/plugin.clap");
    let hold_secs = std::env::var("DAW_TEST_CLAP_GUI_HOLD_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(30);

    let host = ClapHost::default();
    let mut selector = ClapPluginSelector::from_env("DAW_TEST_CLAP_GUI");
    if selector.id.is_none() && selector.required_terms.is_empty() {
        selector = ClapPluginSelector::terms(["parametric", "equalizer", "x32", "stereo"]);
    }
    let (plugin_index, descriptor) = host
        .find_in_bundle(&path, &selector)
        .expect("matching descriptor should exist");
    eprintln!(
        "opening descriptor #{plugin_index}: '{}' id='{}' from {}",
        descriptor.name,
        descriptor.id,
        path.display()
    );

    let result = host
        .smoke_test_gui(&path, &selector, Duration::from_secs(hold_secs))
        .expect("CLAP GUI should open");
    eprintln!(
        "GUI smoke test complete: descriptor #{} '{}' held for {:?}",
        result.plugin_index, result.descriptor.name, result.held_for
    );
}

#[cfg(not(feature = "clap-host"))]
fn main() {
    eprintln!("enable --features clap-host");
    std::process::exit(2);
}

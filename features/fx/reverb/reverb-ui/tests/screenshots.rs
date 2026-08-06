//! Visual-inspection harness: rasterize the real editor to PNGs.
//!
//! The same mount the plugin embeds, painted through `render_png` (anyrender +
//! vello_cpu). Nothing here asserts — a wrong-looking panel is not a failing
//! test, it is a picture you have to look at:
//!
//! ```sh
//! just reverb-shots      # or:
//! cargo test -p reverb-ui --features native --test screenshots
//! ```
//!
//! Output lands in `target/gui-shots/reverb/` (`FTS_SHOTS_DIR` overrides).

#![cfg(feature = "native")]

use std::path::PathBuf;

#[path = "support/mod.rs"]
mod support;

use support::{mount_with, Fixture};

fn shots_dir() -> PathBuf {
    let dir = std::env::var("FTS_SHOTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../target/gui-shots/reverb")
        });
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
    dir
}

fn shot(fx: &Fixture, name: &str) {
    let path = shots_dir().join(format!("{name}.png"));
    fx.tester.render_png(&path);
    println!("shot: {}", path.display());
}

/// Open the editor already on `profile_id`, the way a host restoring a session
/// would — the persisted id is what the editor resolves from.
async fn on_profile(profile_id: &str) -> Fixture {
    let params = std::sync::Arc::new(reverb_ui::params::ReverbParams::default());
    params.store_profile_id(reverb_profiles::profile_index(profile_id).unwrap());
    let mut fx = mount_with(
        params,
        reverb_ui::control_view::EDITOR_W,
        reverb_ui::control_view::EDITOR_H,
    );
    fx.settle().await;
    fx
}

/// One shot per family, plus every variant — the point of the exercise is that
/// they do not look like each other.
#[tokio::test]
async fn every_space() {
    for profile in reverb_profiles::PROFILES {
        let fx = on_profile(profile.id).await;
        shot(&fx, &profile.id.replace('_', "-"));
    }
}

/// The IR panel with a library behind it — the browser is the half of a
/// convolution reverb that is not a knob.
#[tokio::test]
async fn ir_with_a_library() {
    // Safety: the harness is single-threaded per test binary and this runs
    // before the editor mounts.
    unsafe { std::env::set_var("FTS_IR_DIR", concat!(env!("CARGO_MANIFEST_DIR"), "/tests/irs")) };
    let fx = on_profile("ir").await;
    shot(&fx, "ir-library");
}

//! Visual-inspection harness: rasterize the real editor to PNGs.
//!
//! The same mount the plugin embeds, painted through `render_png` (anyrender +
//! vello_cpu). Nothing here asserts — a wrong-looking panel is not a failing
//! test, it is a picture you have to look at:
//!
//! ```sh
//! just modulation-shots      # or:
//! cargo test -p modulation-ui --features native --test screenshots
//! ```
//!
//! Output lands in `target/gui-shots/modulation/` (`FTS_SHOTS_DIR` overrides).

#![cfg(feature = "native")]

use std::path::PathBuf;

#[path = "support/mod.rs"]
mod support;

use support::{mount_with, Fixture};

fn shots_dir() -> PathBuf {
    let dir = std::env::var("FTS_SHOTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../../target/gui-shots/modulation")
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
    let params = std::sync::Arc::new(modulation_ui::params::ModParams::default());
    params.store_profile_id(modulation_profiles::profile_index(profile_id).unwrap());
    let mut fx = mount_with(
        params,
        modulation_ui::control_view::EDITOR_W,
        modulation_ui::control_view::EDITOR_H,
    );
    fx.settle().await;
    fx
}

/// One shot per family, plus every variant — the point of the exercise is
/// that they do not look like each other.
#[tokio::test]
async fn every_space() {
    for profile in modulation_profiles::PROFILES {
        let fx = on_profile(profile.id).await;
        shot(&fx, &profile.id.replace('_', "-"));
    }
}

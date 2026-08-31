//! The preset surfaces, driven the way a person drives them.
//!
//! The browsing rules live in `preset-browser` and the panel in
//! `preset-browser-ui`; what only a mounted editor can answer is whether the
//! reverb actually carries the strip, and whether Browse is reachable. That
//! second half is not a formality: a z-indexed overlay renders on top in blitz
//! but takes no clicks headlessly (see `stack_strip`'s note in comp-ui), which
//! is exactly the failure this catches.
//!
//! The library lives on disk and is usually empty here, so these assert that
//! the surfaces exist and respond — not what is in them.

#![cfg(feature = "native")]

use dioxus_test::by_testid;

#[path = "support/mod.rs"]
mod support;

use support::mount;

#[tokio::test]
async fn the_top_rail_carries_the_preset_strip() -> dioxus_test::Result<()> {
    let mut fx = mount();
    fx.settle().await;

    fx.tester.query(by_testid("preset-bar-name")).immediately()?;
    assert!(
        fx.tester
            .query(by_testid("reverb-presets"))
            .immediately()
            .is_err(),
        "the browser stays shut until asked for",
    );

    fx.tap("preset-bar-browse").await;
    fx.tester.query(by_testid("reverb-presets")).immediately()?;
    Ok(())
}

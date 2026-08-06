//! The IR browser, driven the way a person drives it.
//!
//! Loading an impulse response is the half of a convolution reverb that is not
//! a knob, and it crosses three threads: the click happens on the GUI thread,
//! the decode on a worker, and the swap on the audio thread. These tests cover
//! the first two — the third is the chain's own channel, which it already has
//! tests for.

#![cfg(feature = "native")]

use dioxus_test::by_testid;

#[path = "support/mod.rs"]
mod support;

use support::{mount_with, Fixture};

fn library_dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/irs")
}

async fn ir_editor() -> Fixture {
    unsafe { std::env::set_var("FTS_IR_DIR", library_dir()) };
    let params = std::sync::Arc::new(reverb_ui::params::ReverbParams::default());
    params.store_profile_id(reverb_profiles::profile_index("ir").unwrap());
    let mut fx = mount_with(
        params,
        reverb_ui::control_view::EDITOR_W,
        reverb_ui::control_view::EDITOR_H,
    );
    fx.settle().await;
    fx
}

/// The browser lists what is in the library.
///
/// Only the IR face carries one — a spring has no impulse to load — and that
/// half is asserted against the design table in `faces`, where it is a
/// property of the panel rather than something to go clicking for.
#[tokio::test]
async fn the_browser_lists_the_library() -> dioxus_test::Result<()> {
    let mut fx = ir_editor().await;
    let entries: Vec<_> = fx
        .tester
        .query_all(by_testid("ir-entry"))
        .immediately()
        .into_iter()
        .collect();
    assert!(
        entries.len() >= 4,
        "expected the four test impulses, found {}",
        entries.len(),
    );

    Ok(())
}

/// Clicking a name records the path a session reopens with, and hands the
/// file to the loader.
///
/// The editor deliberately cannot load one itself: decoding is unbounded work.
/// Mounted headless there is no engine on the other end of the channel, and
/// the panel says exactly that rather than pretending it worked — which is
/// also how a real host would report a plugin whose editor outlived its
/// processor.
#[tokio::test]
async fn clicking_an_impulse_records_it_and_hands_it_to_the_loader() -> dioxus_test::Result<()> {
    let mut fx = ir_editor().await;
    assert_eq!(*fx.params.ir_path.read(), "", "nothing is loaded on open");

    let el = fx.tester.query(by_testid("ir-entry")).immediately()?;
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    fx.tester.pointer_down(ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_up(ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    fx.settle().await;

    let path = fx.params.ir_path.read().clone();
    assert!(
        path.starts_with(library_dir()) && path.ends_with(".wav"),
        "the session would reopen with {path:?}",
    );

    // The worker runs to completion, and reports.
    for _ in 0..50 {
        if !fx.ui.ir_loading.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }
    let error = fx.ui.ir_error.lock().clone();
    assert!(
        error.as_deref().unwrap_or_default().contains("no engine"),
        "headless, the load should report that nothing is attached; got {error:?}",
    );
    Ok(())
}

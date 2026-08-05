//! Visual-inspection harness: rasterize the real editor to PNGs.
//!
//! Same mount as the behavioural tests — `comp_ui::control_view::App` on the
//! headless Blitz DOM — but painted through `DocumentTester::render_png`
//! (anyrender + vello_cpu, the CPU path blitz's own screenshot tests use). So
//! these are pixels from the actual editor, not a mock-up, and you can look at
//! a faceplate without opening a DAW.
//!
//! Nothing here asserts. A wrong-looking panel is not a failing test, it is a
//! picture you have to look at:
//!
//! ```sh
//! just comp-shots            # or:
//! cargo test -p comp-ui --features native --test screenshots
//! ```
//!
//! Output lands in `target/gui-shots/comp/` (override with `FTS_SHOTS_DIR`).
//! The shots are taken at the editor's design size, which is what the plugin
//! asks the host for.

#![cfg(feature = "native")]

use std::path::PathBuf;

use dioxus_test::by_testid;

#[path = "support/mod.rs"]
mod support;

use support::{mount_sized, Fixture};

/// Where the PNGs land. `target/gui-shots/comp` by default so they are
/// gitignored and easy to find; `FTS_SHOTS_DIR` overrides it.
fn shots_dir() -> PathBuf {
    let dir = std::env::var("FTS_SHOTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../../target/gui-shots/comp")
        });
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
    dir
}

fn shot(fx: &Fixture, name: &str) {
    let path = shots_dir().join(format!("{name}.png"));
    fx.tester.render_png(&path);
    // Printed so `cargo test -- --nocapture` tells you where to look.
    println!("shot: {}", path.display());
}

/// Mount at the size the plugin shell requests from the host.
fn mount_design() -> Fixture {
    mount_sized(
        comp_ui::control_view::EDITOR_W,
        comp_ui::control_view::EDITOR_H,
    )
}

/// Mount, switch to a profile, and size the window the way the host would:
/// switching profile asks for that face's size, so a shot taken at the FTS
/// surface's size is not what anyone sees.
async fn mount_face(profile_index: usize) -> Fixture {
    let (w, h) = comp_ui::faces::preferred_editor_size(profile_index);
    let mut fx = mount_sized(w, h);
    select_profile(&mut fx, profile_index).await;
    fx
}

/// Click profile `index` in the shell rail — the face switch.
async fn select_profile(fx: &mut Fixture, index: usize) {
    let id = comp_ui::faces::PROFILE_IDS[index];
    let item = fx
        .tester
        .query(by_testid(&format!("rail-item-{id}")))
        .immediately()
        .unwrap_or_else(|e| panic!("rail item {id} missing: {e:?}"));
    let (ox, oy) = item.document_origin();
    let (w, h) = item.size();
    let (x, y) = (ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    fx.tester.pointer_down(x, y);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_up(x, y);
    fx.settle().await;
}

/// Click the centre of whatever carries `testid`.
async fn click_testid(fx: &mut Fixture, testid: &str) {
    let el = fx
        .tester
        .query(by_testid(testid))
        .immediately()
        .unwrap_or_else(|e| panic!("{testid} missing: {e:?}"));
    let (ox, oy) = el.document_origin();
    let (w, h) = el.size();
    let (x, y) = (ox + w as f64 / 2.0, oy + h as f64 / 2.0);
    fx.tester.pointer_down(x, y);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_up(x, y);
    fx.settle().await;
}

/// Turn a hardware knob to a position, so a shot shows a panel in use rather
/// than every control parked at its default.
async fn turn(fx: &mut Fixture, testid: &str, dy: f64) {
    let (x, y) = fx.knob_center(testid);
    fx.tester.pointer_down(x, y);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_move(x, y + dy, true);
    let _ = fx.tester.pump().await;
    fx.tester.pointer_up(x, y + dy);
    fx.settle().await;
}

#[tokio::test]
async fn shot_control_face() {
    let fx = mount_design();
    shot(&fx, "control-basic");
}

#[tokio::test]
async fn shot_control_face_advanced() {
    let mut fx = mount_design();
    click_testid(&mut fx, "advanced-toggle").await;
    shot(&fx, "control-advanced");
}

/// The character dropdown open — seven waveshapes that used to be seven
/// segments across the bar.
#[tokio::test]
async fn shot_character_dropdown() {
    let mut fx = mount_design();
    click_testid(&mut fx, "advanced-toggle").await;
    click_testid(&mut fx, "select-charmode-trigger").await;
    shot(&fx, "control-advanced-dropdown");
}

#[tokio::test]
async fn shot_la2a_face() {
    let mut fx = mount_face(1).await;
    // Some peak reduction on the meter, and gain brought back up.
    fx.ui.gain_reduction_db
        .store(7.5, std::sync::atomic::Ordering::Relaxed);
    turn(&mut fx, "hw-knob-peak-reduction", -35.0).await;
    turn(&mut fx, "hw-knob-gain", -25.0).await;
    shot(&fx, "la2a");
}

#[tokio::test]
async fn shot_ssl_bus_face() {
    let mut fx = mount_face(2).await;
    fx.ui.gain_reduction_db
        .store(4.0, std::sync::atomic::Ordering::Relaxed);
    turn(&mut fx, "hw-knob-threshold", -30.0).await;
    shot(&fx, "ssl-bus");
}

#[tokio::test]
async fn shot_1176_face() {
    let mut fx = mount_face(3).await;
    fx.ui.gain_reduction_db
        .store(11.0, std::sync::atomic::Ordering::Relaxed);
    turn(&mut fx, "hw-knob-input", -30.0).await;
    turn(&mut fx, "hw-knob-attack", -40.0).await;
    shot(&fx, "urei-1176");
}

/// The same face in a window the user dragged wider — this is what host
/// resizing does to a faceplate (scale it, not reflow it).
#[tokio::test]
async fn shot_la2a_face_large() {
    let mut fx = mount_sized(1500, 520);
    select_profile(&mut fx, 1).await;
    fx.ui.gain_reduction_db
        .store(5.0, std::sync::atomic::Ordering::Relaxed);
    shot(&fx, "la2a-large");
}

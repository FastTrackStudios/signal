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

use support::{mount_sized, mount_with, Fixture};

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

/// The stack, three stages deep: FTS surface + LA-2A serial, an 1176 on a
/// parallel lane — every stage visible as a row (`fx.stack.strip`).
#[tokio::test]
async fn shot_stack_of_three() {
    use nice_plug::prelude::Param;
    let (w, base_h) = comp_ui::faces::preferred_editor_size(0);
    let params = std::sync::Arc::new(comp_ui::params::CompParams::default());
    unsafe {
        let set_int = |p: &nice_plug::prelude::IntParam, v: i32| {
            p.as_ptr()
                ._internal_set_normalized_value(p.preview_normalized(v));
        };
        let set_bool = |p: &nice_plug::prelude::BoolParam| {
            p.as_ptr()._internal_set_normalized_value(1.0);
        };
        set_int(
            &params.stage2.profile,
            comp_profiles::profile_index("la2a").unwrap() as i32,
        );
        params
            .stage2
            .store_profile_id(comp_profiles::profile_index("la2a").unwrap());
        set_bool(&params.stage2.in_use);
        set_int(
            &params.stage3.profile,
            comp_profiles::profile_index("urei_1176").unwrap() as i32,
        );
        params
            .stage3
            .store_profile_id(comp_profiles::profile_index("urei_1176").unwrap());
        set_bool(&params.stage3.in_use);
        set_int(&params.stage3.lane, 1);
    }
    // The window the editor would ask the host for: each row at its face's
    // preferred height.
    let (w, h) = comp_ui::faces::stack_editor_size_rows(
        &params,
        &[0, 1, 2],
        fts_audio_ui::EditorForm::default(),
        0,
    );
    let _ = base_h;
    let mut fx = mount_with(params, w, h);
    fx.settle().await;
    shot(&fx, "stack-of-three");
}

/// A stage's sidechain-EQ sidecar, open under the FTS surface
/// (`fx.embed-eq.one-surface`).
#[tokio::test]
async fn shot_sidechain_sidecar() {
    use nice_plug::prelude::Param;
    let (w, base_h) = comp_ui::faces::preferred_editor_size(0);
    let params = std::sync::Arc::new(comp_ui::params::CompParams::default());
    // Pose the curve: kick notch + de-ess boost.
    unsafe {
        let b0 = &params.stage1.sc_eq[0].gain_db;
        b0.as_ptr()
            ._internal_set_normalized_value(b0.preview_normalized(-9.0));
        let b4 = &params.stage1.sc_eq[4].gain_db;
        b4.as_ptr()
            ._internal_set_normalized_value(b4.preview_normalized(7.0));
    }
    let mut fx = mount_with(
        params,
        w + comp_ui::faces::SIDECAR_W as u32,
        base_h,
    );
    fx.settle().await;
    // Open the sidecar through the rail toggle, like a hand would.
    click_testid(&mut fx, "sc-eq-rail-toggle").await;
    shot(&fx, "sidechain-sidecar");
}

/// Mount, switch to a profile, and size the window the way the host would:
/// switching profile asks for that face's size, so a shot taken at the FTS
/// surface's size is not what anyone sees.
async fn mount_face(profile_id: &str) -> Fixture {
    let index = comp_profiles::profile_index(profile_id).unwrap();
    let (w, h) = comp_ui::faces::preferred_editor_size(index);
    let mut fx = mount_sized(w, h);
    select_profile(&mut fx, profile_id).await;
    fx
}

/// Select a profile by id through the shell rail, cycling the family button
/// as many times as it takes.
async fn select_profile(fx: &mut Fixture, profile_id: &str) {
    let (category, _) = comp_profiles::category_of(profile_id)
        .unwrap_or_else(|| panic!("{profile_id} is in no category"));
    let target = comp_profiles::profile_index(profile_id).unwrap() as i32;
    let rail_id = comp_profiles::CATEGORIES[category].id;
    for _ in 0..8 {
        if fx.params.stage1.profile.value() == target {
            return;
        }
        click_testid(fx, &format!("rail-item-{rail_id}")).await;
    }
    panic!("rail never reached {profile_id}");
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
    let mut fx = mount_face("la2a").await;
    // Some peak reduction on the meter, and gain brought back up.
    fx.ui.gain_reduction_db
        .store(7.5, std::sync::atomic::Ordering::Relaxed);
    turn(&mut fx, "hw-knob-peak-reduction", -35.0).await;
    turn(&mut fx, "hw-knob-gain", -25.0).await;
    shot(&fx, "la2a");
}

#[tokio::test]
async fn shot_ssl_bus_face() {
    let mut fx = mount_face("ssl_bus").await;
    fx.ui.gain_reduction_db
        .store(4.0, std::sync::atomic::Ordering::Relaxed);
    turn(&mut fx, "hw-knob-threshold", -30.0).await;
    shot(&fx, "ssl-bus");
}

#[tokio::test]
async fn shot_1176_face() {
    let mut fx = mount_face("urei_1176").await;
    fx.ui.gain_reduction_db
        .store(11.0, std::sync::atomic::Ordering::Relaxed);
    turn(&mut fx, "hw-knob-input", -30.0).await;
    turn(&mut fx, "hw-knob-attack", -40.0).await;
    shot(&fx, "urei-1176");
}

/// Every remaining unit, one shot each — this is the sheet to look at after
/// touching the panel kit.
#[tokio::test]
async fn shot_every_other_unit() {
    for id in [
        "cl1b",
        "fairchild670",
        "manley_vari_mu",
        "dbx160",
        "distressor",
        "urei_1176_silver",
        "urei_1176_ln",
    ] {
        let mut fx = mount_face(id).await;
        fx.ui.gain_reduction_db
            .store(6.0, std::sync::atomic::Ordering::Relaxed);
        fx.ui.output_peak_db
            .store(-14.0, std::sync::atomic::Ordering::Relaxed);
        fx.settle().await;
        shot(&fx, &id.replace('_', "-"));
    }
}

/// Every size preset, on a face that has a panel — this is the sheet that
/// says whether a form is usable, which is not something a size in a table can
/// tell you.
#[tokio::test]
async fn shot_every_editor_form() {
    for form in fts_audio_ui::EDITOR_FORMS {
        let index = comp_profiles::profile_index("la2a").unwrap();
        let (w, h) = comp_ui::faces::editor_size_for(index, *form);
        let params = std::sync::Arc::new(comp_ui::params::CompParams::default());
        params.stage1.store_profile_id(index);
        params.store_editor_form(*form);
        let mut fx = support::mount_with(params, w, h);
        fx.settle().await;
        shot(&fx, &format!("form-{}", form.id().replace('_', "-")));
    }
}

/// The same face in a window the user dragged wider — this is what host
/// resizing does to a faceplate (scale it, not reflow it).
#[tokio::test]
async fn shot_la2a_face_large() {
    let mut fx = mount_sized(1500, 520);
    select_profile(&mut fx, "la2a").await;
    fx.ui.gain_reduction_db
        .store(5.0, std::sync::atomic::Ordering::Relaxed);
    shot(&fx, "la2a-large");
}

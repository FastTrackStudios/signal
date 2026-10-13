//! A generated surface — design *and* controls — survives every storage
//! path: frame markup, a Loro replica, and a `.frame` bundle.

use frame::surface::memory::demo_catalog;
use frame::surface::runtime::Surface;
use frame::surface::widgets::{Options, surface_for_catalog};

fn scene() -> frame::document::proto::Scene {
    surface_for_catalog(&demo_catalog(), &Options::default()).to_scene()
}

#[test]
fn controls_round_trip_through_markup() {
    let s = scene();
    let text = frame::document::text::print(&s);
    assert!(text.contains("control={"), "controls are printed");
    let back = frame::document::text::parse(&text, s.assets.clone()).unwrap();
    assert_eq!(back, s);
    let surface = Surface::from_scene(back);
    assert_eq!(surface.controls.len(), 26);
}

#[test]
fn controls_round_trip_through_loro_and_bundle() {
    let s = scene();
    let replica = frame::document::loro::LoroScene::from_scene(&s);
    assert_eq!(replica.scene(), s);
    let bytes = frame::document::bundle::to_bytes(
        &replica,
        &frame::document::bundle::Manifest::new("Demo rig"),
    )
    .unwrap();
    let (manifest, loaded) = frame::document::bundle::from_bytes(&bytes).unwrap();
    assert_eq!(manifest.title, "Demo rig");
    assert_eq!(loaded.scene(), s);
}

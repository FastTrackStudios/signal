//! A generated surface — design *and* controls — survives every storage
//! path: frame markup, a Loro replica, and a `.frame` bundle.

use surface_memory::demo_catalog;
use surface_runtime::Surface;
use surface_widgets::{Options, surface_for_catalog};

fn scene() -> document_proto::Scene {
    surface_for_catalog(&demo_catalog(), &Options::default()).to_scene()
}

#[test]
fn controls_round_trip_through_markup() {
    let s = scene();
    let text = document_text::print(&s);
    assert!(text.contains("control={"), "controls are printed");
    let back = document_text::parse(&text, s.assets.clone()).unwrap();
    assert_eq!(back, s);
    let surface = Surface::from_scene(back);
    assert_eq!(surface.controls.len(), 26);
}

#[test]
fn controls_round_trip_through_loro_and_bundle() {
    let s = scene();
    let replica = document_loro::LoroScene::from_scene(&s);
    assert_eq!(replica.scene(), s);
    let bytes =
        document_bundle::to_bytes(&replica, &document_bundle::Manifest::new("Demo rig")).unwrap();
    let (manifest, loaded) = document_bundle::from_bytes(&bytes).unwrap();
    assert_eq!(manifest.title, "Demo rig");
    assert_eq!(loaded.scene(), s);
}

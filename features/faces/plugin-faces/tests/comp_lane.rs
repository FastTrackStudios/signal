//! The compressors' lane shapes: every unit has a `-lane` (760 × 164) and
//! a `-tall` (760 × 328) face, each at its shape's size, linting clean
//! against the merged `comp` catalog, binding only the rack face's own
//! addresses and declaring only what it binds.

use document_proto::Scene;
use plugin_faces::{Face, catalogs, lane::Shape};
use surface_proto::Source;

const UNITS: &[&str] = &[
    "01-fet-black",
    "02-fet-silver",
    "03-fet-late",
    "04-opto-leveler",
    "05-opto-comp",
    "06-twin-valve",
    "07-vari-mu",
    "08-bus-comp",
    "09-vca-comp",
    "10-character-comp",
];

fn size(scene: &Scene) -> (f64, f64) {
    let page = scene.pages()[0];
    scene
        .node(page)
        .and_then(|p| p.children.first().copied())
        .and_then(|t| scene.node(t))
        .map_or((0.0, 0.0), |n| (n.size.width, n.size.height))
}

fn comp() -> Vec<Face> {
    plugin_faces::all()
        .into_iter()
        .filter(|f| f.plugin == "comp")
        .collect()
}

#[test]
fn every_unit_has_both_shapes_at_their_size() {
    let faces = comp();
    for unit in UNITS {
        for shape in Shape::ALL {
            let file = format!("{unit}-{}", shape.suffix());
            let face = faces
                .iter()
                .find(|f| f.file == file)
                .unwrap_or_else(|| panic!("{file} missing"));
            let (w, h) = size(&face.scene);
            assert_eq!((w, h), shape.size(), "{file}");
        }
    }
}

#[test]
fn lane_faces_lint_clean_and_bind_the_rack_addresses() {
    let faces = comp();
    let catalog = catalogs(&plugin_faces::all())
        .remove("comp")
        .expect("comp catalog");
    for face in faces
        .iter()
        .filter(|f| f.file.ends_with("-lane") || f.file.ends_with("-tall"))
    {
        let issues = design_lint::lint(&face.scene, &catalog, &design_lint::Config::default());
        let errors: Vec<String> = issues
            .iter()
            .filter(|i| i.severity == design_lint::Severity::Error)
            .map(ToString::to_string)
            .collect();
        assert!(errors.is_empty(), "{}: {}", face.file, errors.join("\n"));
        let rack = face
            .file
            .trim_end_matches("-lane")
            .trim_end_matches("-tall");
        let rack = faces.iter().find(|f| f.file == rack).expect("rack face");
        let mut bound = std::collections::BTreeSet::new();
        for id in face.scene.walk(face.scene.root) {
            let Some(c) = face.scene.node(id).and_then(|n| n.control.as_ref()) else {
                continue;
            };
            match c.binding.as_ref().map(|b| &b.source) {
                Some(Source::Param { addr }) => {
                    assert!(
                        rack.params.iter().any(|p| &p.addr == addr),
                        "{}: {addr} is not the rack face's",
                        face.file
                    );
                    bound.insert(addr.0.clone());
                }
                Some(Source::Stream { id, .. }) => assert!(
                    rack.streams.iter().any(|s| &s.id == id),
                    "{}: stream {} is not the rack face's",
                    face.file,
                    id.0
                ),
                other => panic!("{}: binding {other:?}", face.file),
            }
        }
        for p in &face.params {
            assert!(
                bound.contains(&p.addr.0),
                "{}: declares {} but binds no control to it",
                face.file,
                p.addr
            );
        }
    }
}

//! The FET limiter faces: they lint clean against the merged `comp`
//! catalog, bind only what exists, and wire the unit's behaviour — the GR
//! needle resting on 0 VU, the ratio bank printed opposite to param order.

use frame::design::lint::{Config, Severity, counts, lint};
use frame::document::proto::{NodeId, NodeKind, Scene};
use frame::surface::kit::meters::scale::Card;
use frame::surface::proto::{ButtonMode, Catalog, Control, ControlKind, Source};
use plugin_faces::{Face, catalogs, comp_fet};

fn comp_catalog() -> Catalog {
    catalogs(&plugin_faces::all())
        .remove("comp")
        .expect("comp catalog")
}

fn controls(scene: &Scene) -> Vec<(NodeId, String, Control)> {
    scene
        .walk(scene.root)
        .into_iter()
        .filter_map(|id| scene.node(id))
        .filter_map(|n| n.control.clone().map(|c| (n.id, n.name.clone(), c)))
        .collect()
}

fn text_of(scene: &Scene, id: NodeId) -> Option<String> {
    scene
        .walk(id)
        .into_iter()
        .filter_map(|n| scene.node(n))
        .find_map(|n| match &n.kind {
            NodeKind::Text(t) if n.name == "legend" => Some(t.characters.clone()),
            _ => None,
        })
}

#[test]
fn three_finishes() {
    let files: Vec<&str> = comp_fet::faces().iter().map(|f| f.file).collect();
    assert_eq!(files, ["01-fet-black", "02-fet-silver", "03-fet-late"]);
    assert!(comp_fet::faces().iter().all(|f| f.plugin == "comp"));
}

#[test]
fn every_face_lints_without_errors() {
    let catalog = comp_catalog();
    for face in comp_fet::faces() {
        let issues = lint(&face.scene, &catalog, &Config::default());
        let errors: Vec<String> = issues
            .iter()
            .filter(|i| i.severity == Severity::Error)
            .map(ToString::to_string)
            .collect();
        assert!(
            errors.is_empty(),
            "{}: {} errors\n{}",
            face.file,
            counts(&issues).0,
            errors.join("\n")
        );
    }
}

#[test]
fn every_binding_exists_in_the_catalog() {
    let catalog = comp_catalog();
    for face in comp_fet::faces() {
        let bound = controls(&face.scene);
        assert!(
            bound.len() >= 14,
            "{}: only {} controls",
            face.file,
            bound.len()
        );
        for (_, name, c) in &bound {
            let issues = c.validate(&catalog);
            assert!(issues.is_empty(), "{} / {name}: {issues:?}", face.file);
            match &c.binding.as_ref().expect("bound").source {
                Source::Param { addr } => assert!(
                    face.params.iter().any(|p| p.addr == *addr),
                    "{}: {addr} not declared",
                    face.file
                ),
                Source::Stream { id, .. } => assert!(
                    face.streams.iter().any(|s| s.id == *id),
                    "{}: {id:?} not declared",
                    face.file
                ),
                other => panic!("{}: unexpected source {other:?}", face.file),
            }
        }
        // Everything the face declares is on it, and featured.
        for p in &face.params {
            assert!(p.featured, "{} not featured", p.addr);
            assert!(
                bound.iter().any(|(_, _, c)| matches!(&c.binding.as_ref().unwrap().source, Source::Param { addr } if *addr == p.addr)),
                "{}: {} declared but not bound",
                face.file,
                p.addr
            );
        }
    }
}

fn gauge(face: &Face) -> Control {
    controls(&face.scene)
        .into_iter()
        .map(|(_, _, c)| c)
        .find(|c| matches!(c.kind, ControlKind::Gauge { .. }))
        .expect("a gauge")
}

#[test]
fn the_gr_needle_rests_on_zero_vu_and_swings_left() {
    for face in comp_fet::faces() {
        let g = gauge(&face);
        let ControlKind::Gauge {
            floor_db,
            ceiling_db,
            ..
        } = g.kind
        else {
            unreachable!()
        };
        let b = g.binding.expect("bound");
        assert!(matches!(&b.source, Source::Stream { id, .. } if id.0 == comp_fet::GR));
        // The runtime maps dB linearly onto floor..ceiling, then the curve.
        let pos = |db: f64| b.curve.apply((db - floor_db) / (ceiling_db - floor_db));
        let card = Card::Vu;
        assert!(
            (pos(0.0) - card.fraction(0.0)).abs() < 1e-9,
            "0 dB GR must sit on the printed 0 VU"
        );
        assert!(
            (pos(-5.0) - card.fraction(-5.0)).abs() < 1e-9,
            "5 dB GR must sit on the printed 5"
        );
        assert!(
            pos(-10.0) < pos(-3.0) && pos(-3.0) < pos(0.0),
            "more reduction swings left"
        );
    }
}

#[test]
fn the_ratio_bank_reads_twenty_at_the_top_and_selects_by_param_order() {
    let face = &comp_fet::faces()[0];
    let options = ["4", "8", "12", "20", "All"];
    let mut rows: Vec<(f64, String, u32, u32)> = controls(&face.scene)
        .into_iter()
        .filter(|(_, _, c)| matches!(&c.binding.as_ref().unwrap().source, Source::Param { addr } if addr.0 == comp_fet::RATIO))
        .map(|(id, _, c)| {
            let ControlKind::Button { mode: ButtonMode::Select { index, count } } = c.kind else { panic!("ratio is a select bank") };
            let y = face.scene.node(id).map_or(0.0, |n| n.transform.ty);
            (y, text_of(&face.scene, id).expect("row legend"), index, count)
        })
        .collect();
    // Top to bottom as printed, whatever order the rows were built in.
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    let rows: Vec<(String, u32, u32)> = rows.into_iter().map(|(_, l, i, c)| (l, i, c)).collect();
    assert_eq!(
        rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
        ["All", "20", "12", "8", "4"],
        "top to bottom"
    );
    for (label, index, count) in rows {
        assert_eq!(count, 5);
        assert_eq!(
            options[index as usize], label,
            "the {label} button selects {}",
            options[index as usize]
        );
    }
}

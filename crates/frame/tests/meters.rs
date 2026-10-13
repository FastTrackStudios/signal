//! The meter family: every VU face places, wires one needle about its
//! pivot, scales with its size, lints clean, and reads on its printed marks.

use std::collections::BTreeMap;

use frame::document::proto::{Node, NodeId, NodeKind, Scene};
use frame::surface::kit::meters::scale::{self, Card, PIVOT_X, PIVOT_Y};
use frame::surface::kit::{Family, Place, components, place};
use frame::surface::proto::{
    Binding, Catalog, ControlKind, EntryId, MeterTap, ParamAddr, ParamInfo, ParamValue, Range,
    SlotRole, Source, SourceSelect, StreamFrame, StreamInfo, StreamKind,
};
use frame::surface::runtime::{Runtime, Surface};
use frame::surface::widgets::{Builder, Theme};

fn level() -> Binding {
    Binding {
        source: Source::Stream {
            id: EntryId::new("out_level"),
            channel: Some(0),
        },
        ..Binding::stream("out_level")
    }
}

/// Place `id` in a fresh scene; returns the scene (controls attached) and
/// the component's root.
fn placed(id: &str, p: &Place) -> (Scene, NodeId) {
    let mut scene = Scene::new("Meters");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let mut b = Builder {
        scene: &mut scene,
        controls: &mut controls,
        theme: &theme,
    };
    let holder = b.add(page, Node::new(NodeKind::Frame, "holder"));
    let root = place(&mut b, holder, id, p).unwrap_or_else(|e| panic!("{id}: {e}"));
    for (nid, c) in &controls {
        scene.node_mut(*nid).expect("control node").control = Some(c.clone());
    }
    (scene, root)
}

/// The needle meters (the VU faces).
fn meters() -> Vec<frame::surface::kit::ComponentDef> {
    components()
        .into_iter()
        .filter(|c| c.family == Family::Meter && c.id.starts_with("meter.vu"))
        .collect()
}

#[test]
fn every_meter_and_variant_places_with_one_needle_in_a_pivot_frame() {
    assert!(meters().len() >= 5);
    for def in meters() {
        assert_eq!(def.binds, "level stream");
        let variants: Vec<Option<&str>> = std::iter::once(None)
            .chain(def.variants.iter().map(|v| Some(*v)))
            .collect();
        for v in variants {
            let p = Place {
                variant: v.map(str::to_string),
                ..Place::new("Out").bind(level())
            };
            let (scene, root) = placed(def.id, &p);
            let control = scene
                .node(root)
                .and_then(|n| n.control.clone())
                .expect("root carries the control");
            assert!(
                matches!(control.kind, ControlKind::Gauge { .. }),
                "{} {v:?}",
                def.id
            );
            let needles: Vec<_> = control.slots_of(SlotRole::Needle).collect();
            assert_eq!(needles.len(), 1, "{} {v:?}: one Needle slot", def.id);
            let needle = scene.node(NodeId(needles[0])).expect("needle node");
            let pivot = scene
                .node(needle.parent.expect("needle has a parent"))
                .expect("pivot");
            assert!(
                matches!(pivot.kind, NodeKind::Frame),
                "{}: the needle sits in a frame",
                def.id
            );
            assert!(
                (pivot.size.width - pivot.size.height).abs() < 1e-6,
                "{}: the pivot frame is square",
                def.id
            );
            // The pivot frame is centred on the movement's pivot.
            let card = scene.node(pivot.parent.expect("card")).expect("card");
            let k = card.size.width / scale::VU_W;
            let c = (
                pivot.transform.tx + pivot.size.width / 2.0,
                pivot.transform.ty + pivot.size.height / 2.0,
            );
            assert!(
                (c.0 - PIVOT_X * k).abs() < 1e-6 && (c.1 - PIVOT_Y * k).abs() < 1e-6,
                "{}: pivot at {c:?}",
                def.id
            );
            // Drawn at the centre of travel: straight up, centred on the pivot.
            assert!(needle.transform.rotation().abs() < 1e-9);
            assert!(
                (needle.transform.tx + needle.size.width / 2.0 - pivot.size.width / 2.0).abs()
                    < 1e-6
            );
        }
    }
}

#[test]
fn unknown_variants_are_rejected() {
    let mut scene = Scene::new("x");
    let page = scene.pages()[0];
    let mut controls = BTreeMap::new();
    let theme = Theme::stage_dark();
    let mut b = Builder {
        scene: &mut scene,
        controls: &mut controls,
        theme: &theme,
    };
    assert!(
        place(
            &mut b,
            page,
            "meter.vu_ivory",
            &Place::new("x").variant("red")
        )
        .is_err(),
        "only the backlit face takes a tint"
    );
}

#[test]
fn meters_scale_with_place_size() {
    for def in meters() {
        for size in [140.0, 220.0, 400.0] {
            for v in [None, Some("open")] {
                let p = Place {
                    variant: v.map(str::to_string),
                    ..Place::new("Out").size(size).bind(level())
                };
                let (scene, root) = placed(def.id, &p);
                let n = scene.node(root).expect("root");
                assert!(
                    (n.size.width - size).abs() < 1e-6,
                    "{} {v:?} at {size}: {}",
                    def.id,
                    n.size.width
                );
                assert!(
                    n.size.height > size * 0.5 && n.size.height < size * 0.8,
                    "{}: {}",
                    def.id,
                    n.size.height
                );
            }
        }
    }
}

#[test]
fn meters_lint_clean_bound_to_the_output_level() {
    let catalog = frame::surface::memory::demo_catalog();
    for def in meters() {
        let (scene, _) = placed(def.id, &Place::new("Out").bind(level()));
        let issues =
            frame::design::lint::lint(&scene, &catalog, &frame::design::lint::Config::default());
        let (errors, warnings, _) = frame::design::lint::counts(&issues);
        assert_eq!(
            (errors, warnings),
            (0, 0),
            "{}: {}",
            def.id,
            issues
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// The needle's angle (degrees) with a level of `db` on the meter's own
/// scale (0 = 0 VU) — fed as `db + VU_REFERENCE_DBFS` dBFS.
fn needle_angle(id: &str, variant: Option<&str>, db: f64) -> f64 {
    let p = Place {
        variant: variant.map(str::to_string),
        ..Place::new("Out").bind(level())
    };
    let (scene, root) = placed(id, &p);
    let needle = NodeId(
        scene
            .node(root)
            .and_then(|n| n.control.as_ref())
            .and_then(|c| c.slots_of(SlotRole::Needle).next())
            .expect("needle"),
    );
    let mut rt = Runtime::new(
        Surface::from_scene(scene),
        frame::surface::memory::demo_catalog(),
    );
    let dbfs = db + frame::surface::proto::VU_REFERENCE_DBFS;
    rt.apply_frame(
        StreamFrame {
            id: EntryId::new("out_level"),
            values: vec![dbfs, dbfs],
        },
        0.0,
    );
    rt.frame(0.0)
        .node(needle)
        .expect("needle")
        .transform
        .rotation()
        .to_degrees()
}

/// The angle at which a tick for `v` is printed, measured off its drawn
/// position about the pivot.
fn printed_angle(card: Card, v: f64) -> f64 {
    let (x, y) = scale::point(card, v, 6.0);
    (x - PIVOT_X).atan2(PIVOT_Y - y).to_degrees()
}

#[test]
fn the_needle_lands_on_the_printed_vu_marks() {
    for db in [-20.0, -10.0, -7.0, -5.0, 0.0, 3.0] {
        let got = needle_angle("meter.vu_ivory", None, db);
        let want = printed_angle(Card::Vu, db);
        assert!(
            (got - want).abs() < 0.5,
            "{db} dB: needle at {got:.2}°, printed at {want:.2}°"
        );
    }
    // 0 VU sits about three-quarters of the way across, not at the middle.
    assert!(needle_angle("meter.vu_amber", None, 0.0) > 10.0);
}

#[test]
fn the_decibel_card_reads_linearly() {
    for db in [-40.0, -20.0, 0.0, 20.0] {
        let got = needle_angle("meter.vu_amber_blue", None, db);
        let want = printed_angle(Card::Decibels, db);
        assert!(
            (got - want).abs() < 0.5,
            "{db} dB: needle at {got:.2}°, printed at {want:.2}°"
        );
    }
    // A card variant rewires the law with the print.
    let got = needle_angle("meter.vu_ivory", Some("db"), -10.0);
    assert!((got - printed_angle(Card::Decibels, -10.0)).abs() < 0.5);
}

/// Names of the nodes under `root` that give off light.
fn lights(id: &str, variant: Option<&str>) -> Vec<String> {
    let p = Place {
        variant: variant.map(str::to_string),
        ..Place::new("Out").bind(level())
    };
    let (scene, root) = placed(id, &p);
    scene
        .walk(root)
        .into_iter()
        .filter_map(|n| {
            scene
                .node(n)
                .filter(|n| n.emission.is_some())
                .map(|n| n.name.clone())
        })
        .collect()
}

#[test]
fn lit_meters_give_off_light_where_the_lamp_shows() {
    // The wash glows gently; the hotspot only tints (its light would wash out the print).
    assert_eq!(lights("meter.vu_amber", None), vec!["lamp"]);
    assert!(
        lights("meter.vu_ivory", None).is_empty(),
        "a white lamp only tints its card"
    );
    assert_eq!(
        lights("meter.vu_ivory", Some("lit_amber")),
        vec!["card"],
        "a lit frame glows round the opening"
    );
    assert_eq!(
        lights("meter.vu_backlit", Some("red")),
        vec!["lamp", "needle core"]
    );
}

#[test]
fn a_backlit_needle_is_a_turning_group_of_halo_lines() {
    let (scene, root) = placed("meter.vu_backlit", &Place::new("Out").bind(level()));
    let control = scene
        .node(root)
        .and_then(|n| n.control.clone())
        .expect("control");
    let needle = scene
        .node(NodeId(
            control.slots_of(SlotRole::Needle).next().expect("needle"),
        ))
        .expect("needle node");
    assert!(matches!(needle.kind, NodeKind::Frame));
    let names: Vec<_> = needle
        .children
        .iter()
        .map(|c| scene.node(*c).expect("child").name.as_str())
        .collect();
    assert_eq!(names, ["halo 0", "halo 1", "halo 2", "needle core"]);
    let widths: Vec<f64> = needle
        .children
        .iter()
        .map(|c| scene.node(*c).expect("child").size.width)
        .collect();
    assert!(
        widths.windows(2).all(|w| w[0] > w[1]),
        "widest halo first: {widths:?}"
    );
}

/// The demo catalog plus a gain-reduction stream and a meter switch.
fn catalog() -> Catalog {
    let mut c = frame::surface::memory::demo_catalog();
    c.streams.push(StreamInfo {
        id: EntryId::new("comp/gr"),
        name: "GR".into(),
        kind: StreamKind::GainReduction,
        rate_hz: 30.0,
    });
    c.params.push(
        ParamInfo::new("comp/meter", "Meter", Range::default()).choice(&["GR", "+4", "+8", "Off"]),
    );
    c
}

/// A runtime over one placed component, and its root.
fn running(id: &str, p: &Place) -> (Runtime, NodeId) {
    let (scene, root) = placed(id, p);
    (Runtime::new(Surface::from_scene(scene), catalog()), root)
}

/// Which of a lit meter's lamps (its `Led` slots, in order) are lit.
fn lamps(rt: &Runtime, root: NodeId) -> Vec<bool> {
    let f = rt.frame(0.0);
    let c = &rt.surface().controls[&root];
    c.slots_of(SlotRole::Led)
        .map(|id| {
            let n = f.node(NodeId(id)).expect("lamp");
            let lit = n.opacity > 0.5;
            assert_eq!(lit, n.emission.is_some(), "only a lit lamp gives off light");
            lit
        })
        .collect()
}

fn feed(rt: &mut Runtime, stream: &str, db: f64) {
    rt.apply_frame(
        StreamFrame {
            id: EntryId::new(stream),
            values: vec![db, db],
        },
        0.0,
    );
}

fn lines(issues: &[frame::design::lint::Issue]) -> String {
    issues
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn lit_meters_place_lint_clean_and_glow_from_their_lamps() {
    for id in ["meter.led_bar", "meter.led_column", "meter.led_ladder"] {
        for p in [
            Place::new("Out").bind(level()),
            Place::new("Out")
                .bind(level())
                .options(&["vertical"])
                .reversed(),
        ] {
            let (scene, root) = placed(id, &p);
            let c = scene
                .node(root)
                .and_then(|n| n.control.clone())
                .expect("control");
            assert!(
                matches!(c.kind, ControlKind::Gauge { sweep_degrees, .. } if sweep_degrees == 0.0),
                "{id}"
            );
            let leds: Vec<_> = c.slots_of(SlotRole::Led).collect();
            assert!(leds.len() >= 6, "{id}: {} lamps", leds.len());
            assert!(
                leds.iter()
                    .all(|l| scene.node(NodeId(*l)).is_some_and(|n| n.emission.is_some())),
                "{id}: every lamp can light"
            );
            let issues = frame::design::lint::lint(
                &scene,
                &catalog(),
                &frame::design::lint::Config::default(),
            );
            let (errors, warnings, _) = frame::design::lint::counts(&issues);
            assert_eq!((errors, warnings), (0, 0), "{id}: {}", lines(&issues));
        }
    }
}

#[test]
fn a_bar_lights_to_the_level_and_nothing_without_one() {
    let p = Place::new("Out")
        .bind(level())
        .options(&["floor=-40", "ceiling=3", "segments=16"]);
    for id in ["meter.led_bar", "meter.led_column"] {
        let (mut rt, root) = running(id, &p);
        assert!(
            lamps(&rt, root).iter().all(|l| !l),
            "{id}: dark with no signal"
        );
        // −18.5 dB is halfway along −40…+3.
        feed(&mut rt, "out_level", -18.5);
        let lit = lamps(&rt, root);
        assert_eq!(lit.iter().filter(|l| **l).count(), 8, "{id}: {lit:?}");
        assert!(lit[..8].iter().all(|l| *l), "{id}: from the start");
    }
    // The binding's curve reshapes it.
    let curved = Place {
        binding: Some(Binding {
            curve: frame::surface::proto::Curve::Power { exponent: 2.0 },
            ..level()
        }),
        ..p
    };
    let (mut rt, root) = running("meter.led_bar", &curved);
    feed(&mut rt, "out_level", -18.5);
    assert_eq!(
        lamps(&rt, root).iter().filter(|l| **l).count(),
        4,
        "a quarter through a square law"
    );
}

#[test]
fn a_ladder_lights_each_lamp_at_its_printed_threshold() {
    for (db, want) in [
        (-15.0, [true, false, false, false, false, false]),
        (-4.0, [true, true, true, false, false, false]),
        (3.5, [true; 6]),
    ] {
        let (mut rt, root) = running("meter.led_ladder", &Place::new("Out").bind(level()));
        assert_eq!(lamps(&rt, root), [false; 6]);
        feed(&mut rt, "out_level", db);
        assert_eq!(lamps(&rt, root), want, "{db} dB");
    }
}

#[test]
fn a_gain_reduction_ladder_lights_as_reduction_deepens() {
    let p = Place::new("GR")
        .bind(Binding::stream("comp/gr"))
        .options(&["gr"])
        .reversed();
    let (mut rt, root) = running("meter.led_ladder", &p);
    assert!(
        lamps(&rt, root).iter().all(|l| !l),
        "no reduction, no lamps"
    );
    feed(&mut rt, "comp/gr", -5.0);
    assert_eq!(
        lamps(&rt, root),
        [true, true, true, true, false, false, false, false, false],
        "1, 2, 3, 4 dB"
    );
    // Reversed: lamp 0 (1 dB) is the rightmost.
    let ctl = rt.surface().controls[&root].clone();
    let xs: Vec<f64> = ctl
        .slots_of(SlotRole::Led)
        .map(|id| rt.surface().scene.node(NodeId(id)).unwrap().transform.tx)
        .collect();
    assert!(xs.windows(2).all(|w| w[0] > w[1]), "right to left: {xs:?}");
}

/// A VU's needle angle, in degrees.
fn angle(rt: &Runtime, root: NodeId) -> f64 {
    let needle = NodeId(
        rt.surface().controls[&root]
            .slots_of(SlotRole::Needle)
            .next()
            .unwrap(),
    );
    rt.frame(0.0)
        .node(needle)
        .unwrap()
        .transform
        .rotation()
        .to_degrees()
}

#[test]
fn a_vu_on_gain_reduction_rests_on_zero_and_swings_left() {
    let (mut rt, root) = running(
        "meter.vu_ivory",
        &Place::new("GR").bind(Binding::stream("comp/gr")),
    );
    let zero = printed_angle(Card::Vu, 0.0);
    assert!(
        (angle(&rt, root) - zero).abs() < 0.5,
        "rests on 0 VU: {}",
        angle(&rt, root)
    );
    feed(&mut rt, "comp/gr", -5.0);
    assert!(
        (angle(&rt, root) - printed_angle(Card::Vu, -5.0)).abs() < 0.5,
        "5 dB of reduction on the printed 5"
    );
}

#[test]
fn a_switched_vu_reads_what_its_meter_switch_selects() {
    let taps = vec![
        Some(MeterTap::new("comp/gr")),
        Some(MeterTap::new("out_level").offset(-4.0)),
        Some(MeterTap::new("out_level").offset(-8.0)),
        None,
    ];
    let p = Place::new("VU")
        .bind(Binding::stream("comp/gr"))
        .select(SourceSelect::new("comp/meter", taps));
    let (mut rt, root) = running("meter.vu_ivory", &p);
    feed(&mut rt, "comp/gr", -7.0);
    // +1 dB over the −18 dBFS reference: +1 VU on the plain 0 VU range.
    feed(&mut rt, "out_level", -17.0);
    let select = |rt: &mut Runtime, i: f64| {
        rt.apply_values([ParamValue {
            addr: ParamAddr::new("comp/meter"),
            value: i / 3.0,
        }])
    };
    for (i, db, what) in [
        (0.0, -7.0, "GR"),
        (1.0, -3.0, "+4: +1 out reads −3"),
        (2.0, -7.0, "+8: reads −7"),
        (3.0, -20.0, "off: at the stop"),
    ] {
        select(&mut rt, i);
        assert!(
            (angle(&rt, root) - printed_angle(Card::Vu, db)).abs() < 0.5,
            "{what}"
        );
    }
}

#[test]
fn zero_vu_is_minus_18_dbfs_and_gain_reduction_ignores_it() {
    let (mut rt, root) = running("meter.vu_ivory", &Place::new("Out").bind(level()));
    feed(&mut rt, "out_level", -18.0);
    assert!(
        (angle(&rt, root) - printed_angle(Card::Vu, 0.0)).abs() < 0.5,
        "−18 dBFS reads 0 VU: {}",
        angle(&rt, root)
    );
    let (mut rt, root) = running("meter.vu_ivory", &Place::new("Out").bind(level()));
    feed(&mut rt, "out_level", -25.0);
    assert!(
        (angle(&rt, root) - printed_angle(Card::Vu, -7.0)).abs() < 0.5,
        "−25 dBFS reads −7 VU"
    );
    let (mut rt, root) = running(
        "meter.vu_ivory",
        &Place::new("GR").bind(Binding::stream("comp/gr")),
    );
    feed(&mut rt, "comp/gr", -3.0);
    assert!(
        (angle(&rt, root) - printed_angle(Card::Vu, -3.0)).abs() < 0.5,
        "3 dB of reduction reads −3"
    );
}

#[test]
fn a_stream_lamp_brightens_with_its_value() {
    let (mut rt, root) = running(
        "meter.lamp",
        &Place::new("Tempo")
            .bind(Binding::stream("tempo"))
            .legend("Rate"),
    );
    let glow = NodeId(
        rt.surface().controls[&root]
            .slots_of(SlotRole::Glow)
            .next()
            .expect("a Glow slot"),
    );
    assert!(rt.surface().scene.node(glow).unwrap().emission.is_some());
    assert_eq!(
        rt.frame(0.0).node(glow).unwrap().opacity,
        0.0,
        "dark with no data"
    );
    rt.apply_frame(
        StreamFrame {
            id: EntryId::new("tempo"),
            values: vec![140.0],
        },
        0.0,
    );
    assert!(
        (rt.frame(0.0).node(glow).unwrap().opacity - 0.5).abs() < 1e-9,
        "140 of 40…240 BPM"
    );
    let issues = frame::design::lint::lint(
        &rt.surface().to_scene(),
        rt.catalog(),
        &frame::design::lint::Config::default(),
    );
    assert_eq!(
        frame::design::lint::counts(&issues).0,
        0,
        "{}",
        lines(&issues)
    );
}

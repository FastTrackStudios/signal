use super::*;

fn book() -> &'static FontBook {
    FontBook::bundled()
}

#[test]
fn every_bundled_family_reads_and_names_itself() {
    for fam in BUNDLED {
        for bytes in fam.files {
            let own = Face::read(FontBytes::Static(bytes), 0, None).expect(fam.name);
            assert_eq!(
                fold(&own.family),
                fold(fam.name),
                "{} names itself {:?}",
                fam.name,
                own.family
            );
            let (lo, hi) = own.weight;
            assert!(
                lo >= f32::from(fam.weights.0) - 1.0 && hi <= f32::from(fam.weights.1) + 1.0,
                "{}: weight {lo}..{hi}",
                fam.name
            );
        }
        assert!(book().has_family(fam.name));
    }
    assert_eq!(book().families().len(), BUNDLED.len());
}

#[test]
fn lookup_by_family_is_forgiving() {
    for name in [
        "oswald",
        "OSWALD",
        "Oswald",
        "jetbrains-mono",
        "JetBrainsMono",
        "dseg7",
        "monospace",
    ] {
        assert!(book().has_family(name), "{name}");
    }
    let r = book()
        .resolve(&FontQuery::family("jetbrains mono"))
        .unwrap();
    assert_eq!(r.face.family, "JetBrains Mono");
    let r = book().resolve(&FontQuery::family("serif")).unwrap();
    assert_eq!(r.face.family, "Playfair Display");
}

#[test]
fn missing_family_falls_back_to_inter() {
    let r = book()
        .resolve(&FontQuery::family("Helvetica Neue").weight(700.0))
        .unwrap();
    assert_eq!(r.face.family, "Inter");
    assert!(!r.synthetic_bold, "Inter has a wght axis");
    assert!(!r.coords.is_empty(), "bold is an instance, not the default");
    let r = book().resolve(&FontQuery::default()).unwrap();
    assert_eq!(r.face.family, "Inter");
    assert!(r.coords.is_empty(), "regular Inter is the default instance");
}

#[test]
fn glyph_fallback_walks_the_chain() {
    // DSEG7 draws digits and a few letters but no accents: those come from Inter.
    let chain = book().chain(&FontQuery::family("DSEG7 Classic"));
    assert_eq!(chain.faces[0].face.family, "DSEG7 Classic");
    assert_eq!(chain.pick('8').unwrap().0.face.family, "DSEG7 Classic");
    assert_eq!(chain.pick('é').unwrap().0.face.family, "Inter");
    // A character nobody has gets the primary face's missing glyph.
    let (f, g) = chain.pick('\u{10FFFD}').unwrap();
    assert_eq!((f.face.family.as_str(), g.to_u32()), ("DSEG7 Classic", 0));
}

#[test]
fn weight_picks_static_faces_and_instances_variable_ones() {
    let bold = book()
        .resolve(&FontQuery::family("DSEG7 Classic").weight(700.0))
        .unwrap();
    let light = book()
        .resolve(&FontQuery::family("DSEG7 Classic").weight(300.0))
        .unwrap();
    assert_eq!(bold.face.weight, (700.0, 700.0));
    assert_eq!(light.face.weight, (300.0, 300.0));
    let italic = book()
        .resolve(
            &FontQuery::family("DSEG7 Classic")
                .weight(700.0)
                .italic(true),
        )
        .unwrap();
    assert!(italic.face.italic && italic.face.weight.0 == 700.0 && !italic.synthetic_italic);
    // Inter italic is a face of its own; Oswald has none, so it is slanted.
    let i = book()
        .resolve(&FontQuery::family("Inter").italic(true))
        .unwrap();
    assert!(i.face.italic && !i.synthetic_italic);
    let o = book()
        .resolve(&FontQuery::family("Oswald").italic(true))
        .unwrap();
    assert!(o.synthetic_italic);
    // A static regular asked for bold is emboldened.
    let m = book()
        .resolve(&FontQuery::family("Michroma").weight(800.0))
        .unwrap();
    assert!(m.synthetic_bold);
}

fn area(p: &BezPath) -> f64 {
    use kurbo::Shape;
    p.area().abs()
}

#[test]
fn weight_changes_outlines_not_just_position() {
    let q = FontQuery::family("Inter");
    let (regular, black) = (
        book().resolve(&q).unwrap(),
        book().resolve(&q.weight(900.0)).unwrap(),
    );
    let gid = regular.glyph('H').unwrap();
    let (a, b) = (
        regular.outline(gid, 100.0).unwrap(),
        black.outline(gid, 100.0).unwrap(),
    );
    // Same glyph, same contours, heavier stems: more ink, a different
    // shape (not the regular outline shifted).
    assert!(
        area(&b) > area(&a) * 1.4,
        "black H {} vs regular {}",
        area(&b),
        area(&a)
    );
    use kurbo::Shape;
    let (ba, bb) = (a.bounding_box(), b.bounding_box());
    assert!((bb.width() - ba.width()).abs() > 2.0);
    assert!(black.advance(gid, 100.0) > regular.advance(gid, 100.0));
}

#[test]
fn width_axis_narrows_saira() {
    let q = FontQuery::family("Saira");
    let (normal, condensed) = (book().chain(&q), book().chain(&q.width(62.5)));
    assert!(
        condensed.advance("CONDENSED", 20.0, 0.0) < normal.advance("CONDENSED", 20.0, 0.0) * 0.85
    );
}

#[test]
fn style_names_read_like_figma_and_css() {
    assert_eq!(
        parse_style("Semi Bold Italic"),
        StyleHints {
            weight: Some(600.0),
            italic: true,
            width: None
        }
    );
    assert_eq!(
        parse_style("ExtraBold"),
        StyleHints {
            weight: Some(800.0),
            italic: false,
            width: None
        }
    );
    assert_eq!(
        parse_style("Condensed Light"),
        StyleHints {
            weight: Some(300.0),
            italic: false,
            width: Some(75.0)
        }
    );
    assert_eq!(
        parse_style("italic"),
        StyleHints {
            weight: None,
            italic: true,
            width: None
        }
    );
    assert_eq!(
        parse_style("Regular"),
        StyleHints {
            weight: Some(400.0),
            italic: false,
            width: None
        }
    );
}

#[test]
fn a_run_inherits_what_it_leaves_unset() {
    let node = FontSpec {
        family: Some("Oswald".into()),
        weight: Some(700.0),
        style: Some("italic".into()),
        width: None,
    };
    let run = FontSpec {
        weight: Some(300.0),
        ..FontSpec::default()
    };
    let q = run.over(&node);
    assert_eq!(
        q.query(),
        FontQuery {
            family: Some("Oswald"),
            weight: 300.0,
            italic: true,
            width: 100.0
        }
    );
    let run = FontSpec {
        style: Some("Black".into()),
        ..FontSpec::default()
    };
    assert_eq!(run.over(&node).query().weight, 900.0);
}

#[test]
fn registered_fonts_override_bundled_ones() {
    let mut b = book().clone();
    let before = b.resolve(&FontQuery::family("Inter")).unwrap().face.id();
    b.register_as(Some("Inter"), FontBytes::Static(bundled::INTER));
    let after = b.resolve(&FontQuery::family("Inter")).unwrap().face.id();
    assert_ne!(before, after);
    let single = FontBook::single(FontBytes::Static(INTER));
    assert_eq!(single.fallback(), ["Inter"]);
    assert_eq!(
        single
            .resolve(&FontQuery::family("Oswald"))
            .unwrap()
            .face
            .family,
        "Inter"
    );
}

#[test]
fn chain_line_matches_its_advance() {
    let chain = book().chain(&FontQuery::family("Oswald").weight(600.0));
    let (path, w) = chain.line("GAIN 7.5", 18.0, 1.0);
    assert!(!path.elements().is_empty());
    assert!((w - chain.advance("GAIN 7.5", 18.0, 1.0)).abs() < 1e-9);
}

//! Kit lettering: the family names the kit offers are the ones renderers
//! bundle, and a lettered node survives markup.

use frame::document::proto::{NodeKind, TextAlign};
use frame::surface::kit::draw::{self, Lettering, family};

#[test]
fn every_kit_family_is_bundled() {
    let book = frame::render::FontBook::bundled();
    for name in [
        family::INTER,
        family::OSWALD,
        family::SAIRA,
        family::MICHROMA,
        family::ORBITRON,
        family::DANCING_SCRIPT,
        family::PACIFICO,
        family::CAVEAT_BRUSH,
        family::KNEWAVE,
        family::PLAYFAIR_DISPLAY,
        family::DSEG7,
        family::DSEG14,
        family::DOTO,
        family::VT323,
        family::JETBRAINS_MONO,
        family::UNIFRAKTUR_COOK,
        family::PIRATA_ONE,
    ] {
        assert_eq!(book.family_name(name), Some(name), "{name}");
    }
}

#[test]
fn lettering_sets_family_weight_italic_and_width() {
    let n = draw::styled_text(
        "logo",
        "Blackout",
        0.0,
        0.0,
        200.0,
        40.0,
        "#ffffff",
        TextAlign::Center,
        &Lettering::family(family::SAIRA)
            .weight(800.0)
            .italic()
            .width(62.5),
    );
    let NodeKind::Text(t) = &n.kind else {
        panic!("text")
    };
    assert_eq!(t.font_family.as_deref(), Some("Saira"));
    assert_eq!(t.font_weight, Some(800.0));
    assert_eq!(t.font_style.as_deref(), Some("italic"));
    assert_eq!(t.font_width, Some(62.5));
    let q = frame::render::fonts::text_font(t);
    assert_eq!(q.query().width, 62.5);
    assert!(q.query().italic);

    // Markup keeps every setting.
    let mut scene = frame::document::proto::Scene::new("Page");
    let page = scene.pages()[0];
    let mut n = n;
    n.parent = Some(page);
    let id = n.id;
    scene.node_mut(page).unwrap().children.push(id);
    scene.nodes.insert(id, n);
    let text = frame::document::text::print(&scene);
    assert!(
        text.contains("font=\"Saira\"")
            && text.contains("style=\"italic\"")
            && text.contains("stretch=62.5"),
        "{text}"
    );
    let back =
        frame::document::text::parse(&text, std::collections::BTreeMap::new()).expect("parses");
    let NodeKind::Text(t2) = &back.nodes.values().find(|n| n.name == "logo").unwrap().kind else {
        panic!()
    };
    assert_eq!(
        (t2.font_weight, t2.font_width, t2.font_style.as_deref()),
        (Some(800.0), Some(62.5), Some("italic"))
    );
}

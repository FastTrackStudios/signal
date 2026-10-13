//! Hand-written markup (what a model emits) and the errors it gets back.

use std::collections::BTreeMap;

use document_proto::*;
use document_text::{ParseError, parse, parse_fragment, print};

fn err(src: &str) -> ParseError {
    match parse(src, BTreeMap::new()) {
        Ok(_) => panic!("expected an error for:\n{src}"),
        Err(e) => e,
    }
}

fn frag_err(src: &str) -> ParseError {
    match parse_fragment(src) {
        Ok(_) => panic!("expected an error for:\n{src}"),
        Err(e) => e,
    }
}

const PANEL: &str = r##"
<Document>
  <Page name="Surface">
    <Frame name="Panel" x=0 y=0 w=320 h=200 fill="#1e1e1e" radius=8 layout="row" gap=8 padding=12 align="center">
      <Ellipse name="Knob" w=64 h=64 fill="#ff5a36" stroke="#000000" stroke-weight=2 />
      <Text name="Label" w=64 h=16 size=12 fill="#ffffff">Gain</Text>
    </Frame>
  </Page>
</Document>
"##;

#[test]
fn authored_panel() {
    let s = parse(PANEL, BTreeMap::new()).unwrap();
    let page = s.node(s.pages()[0]).unwrap();
    assert_eq!(page.name, "Surface");
    let panel = s.node(page.children[0]).unwrap();
    assert_eq!(panel.kind, NodeKind::Frame);
    assert_eq!(
        panel.size,
        Size {
            width: 320.0,
            height: 200.0
        }
    );
    assert_eq!(panel.corner_radii, CornerRadii::uniform(8.0));
    let l = panel.layout.as_ref().unwrap();
    assert_eq!(l.direction, LayoutDirection::Horizontal);
    assert_eq!((l.gap, l.padding_left, l.padding_bottom), (8.0, 12.0, 12.0));
    assert_eq!(l.cross_align, Some(CrossAxisAlign::Center));
    let knob = s.node(panel.children[0]).unwrap();
    assert_eq!(knob.parent, Some(panel.id));
    assert_eq!(
        knob.fills,
        vec![Paint::solid(Color::rgba(
            1.0,
            90.0 / 255.0,
            54.0 / 255.0,
            1.0
        ))]
    );
    assert_eq!(knob.stroke.weight, 2.0);
    let label = s.node(panel.children[1]).unwrap();
    let t = label.text().unwrap();
    assert_eq!((t.characters.as_str(), t.font_size), ("Gain", 12.0));
    // Generated ids survive a print → parse cycle.
    assert_eq!(parse(&print(&s), BTreeMap::new()).unwrap(), s);
}

#[test]
fn default_names_and_lenient_values() {
    let s = parse(
        r##"<Document><Page><Rectangle w="10" h={20} fill=#f00 locked /><Text>
            Hello
            world
        </Text></Page></Document>"##,
        BTreeMap::new(),
    )
    .unwrap();
    let page = s.node(s.pages()[0]).unwrap();
    let rect = s.node(page.children[0]).unwrap();
    assert_eq!(rect.name, "Rectangle");
    assert_eq!(
        rect.size,
        Size {
            width: 10.0,
            height: 20.0
        }
    );
    assert!(rect.locked);
    assert_eq!(
        rect.fills,
        vec![Paint::solid(Color::rgba(1.0, 0.0, 0.0, 1.0))]
    );
    let text = s.node(page.children[1]).unwrap();
    assert_eq!(text.text().unwrap().characters, "Hello world");
}

#[test]
fn json_values_accept_shorthand() {
    let s = parse(
        r##"<Document><Page><Rectangle fill={["#ff0000", {"kind":{"LinearGradient":{"stops":[{"position":0,"color":"#000"},{"position":1,"color":"#fff"}]}},"opacity":0.5}]} effects={[{"kind":"DropShadow","color":"#00000040","offset_y":2,"radius":4}]} /></Page></Document>"##,
        BTreeMap::new(),
    )
    .unwrap();
    let rect = s.node(s.node(s.pages()[0]).unwrap().children[0]).unwrap();
    assert_eq!(rect.fills.len(), 2);
    assert_eq!(rect.fills[1].opacity, 0.5);
    assert!(rect.fills[1].visible);
    assert_eq!(rect.effects[0].offset_y, 2.0);
    assert!(rect.effects[0].visible);
}

#[test]
fn unknown_attribute_suggests() {
    let e = err("<Document>\n  <Page>\n    <Rectangle colour=\"#fff\" />\n  </Page>\n</Document>");
    assert_eq!((e.line, e.col), (3, 16));
    assert!(
        e.message
            .contains("unknown attribute `colour` on <Rectangle> — did you mean `fill`?"),
        "{e}"
    );
    let e = err("<Document><Page><Frame widht=4 /></Page></Document>");
    assert!(e.message.contains("did you mean `w`?"), "{e}");
    let e = err("<Document><Page><Frame border-radius=4 /></Page></Document>");
    assert!(e.message.contains("did you mean `radius`?"), "{e}");
}

#[test]
fn element_specific_attributes() {
    let e = err("<Document><Page><Rectangle size=12 /></Page></Document>");
    assert!(e.message.contains("only valid on <Text>"), "{e}");
    let e = err("<Document><Page><Frame gap=8 /></Page></Document>");
    assert!(e.message.contains("layout=\"row\""), "{e}");
}

#[test]
fn unknown_element_suggests() {
    let e = err("<Document><Page><Rect w=4 /></Page></Document>");
    assert!(
        e.message
            .contains("unknown element <Rect> — did you mean <Rectangle>?"),
        "{e}"
    );
    let e = err("<Document><Page><Circle /></Page></Document>");
    assert!(e.message.contains("<Ellipse>"), "{e}");
}

#[test]
fn structural_errors() {
    let e = err("<Document>\n<Page>\n</Frame>\n</Document>");
    assert_eq!(e.line, 3);
    assert!(
        e.message
            .contains("</Frame> doesn't match <Page> opened at 2:1"),
        "{e}"
    );
    let e = err("<Document><Page>");
    assert!(e.message.contains("never closed"), "{e}");
    let e = err("<Document><Frame>Hi</Frame></Document>");
    assert!(e.message.contains("only <Text> holds text"), "{e}");
    let e = err("<Document /><Document />");
    assert!(e.message.contains("exactly one root"), "{e}");
    let e = err("<Document><Page><Frame fill=\"red\" /></Page></Document>");
    assert!(e.message.contains("not a color"), "{e}");
    let e = err("<Document><Page><Frame blend=\"mulitply\" /></Page></Document>");
    assert!(e.message.contains("\"multiply\""), "{e}");
    let e = err("<Document><Page><Frame w=wide /></Page></Document>");
    assert!(e.message.contains("expected a number"), "{e}");
}

#[test]
fn json_error_is_positioned() {
    let e = err("<Document>\n  <Frame fill={[{\"kind\": }]} />\n</Document>");
    assert_eq!(e.line, 2);
    assert!(e.col > 15, "{e}");
    assert!(e.message.contains("invalid JSON in `fill={…}`"), "{e}");
}

#[test]
fn duplicate_ids() {
    let id = NodeId::new();
    let e = err(&format!(
        "<Document><Frame id=\"{id}\" /><Frame id=\"{id}\" /></Document>"
    ));
    assert!(e.message.contains("duplicate id"), "{e}");
    let e = err("<Document id=\"nope\" />");
    assert!(e.message.contains("not a UUID"), "{e}");
}

#[test]
fn fragments() {
    let nodes = parse_fragment(
        "<Frame name=\"A\" layout=\"column\">\n  <Text>One</Text>\n  <Text>Two</Text>\n</Frame>\n<Ellipse name=\"B\" />",
    )
    .unwrap();
    assert_eq!(nodes.len(), 4);
    assert_eq!(nodes[0].parent, None);
    assert_eq!(nodes[0].children, vec![nodes[1].id, nodes[2].id]);
    assert_eq!(nodes[1].parent, Some(nodes[0].id));
    assert_eq!(nodes[3].name, "B");
    assert_eq!(nodes[3].parent, None);

    let e = frag_err("<Page><Frame /></Page>");
    assert!(e.message.contains("can't appear in a fragment"), "{e}");
    assert!(frag_err("  ").message.contains("empty fragment"));
}

/// Every ```jsx example in REFERENCE.md parses (and round-trips).
#[test]
fn reference_examples_parse() {
    let doc = include_str!("../REFERENCE.md");
    let mut count = 0;
    for block in doc.split("```jsx\n").skip(1) {
        let src = block.split("```").next().unwrap();
        if src.trim_start().starts_with("<Document") {
            let s = parse(src, BTreeMap::new()).unwrap_or_else(|e| panic!("{e}\n{src}"));
            assert_eq!(parse(&print(&s), BTreeMap::new()).unwrap(), s);
        } else {
            parse_fragment(src).unwrap_or_else(|e| panic!("{e}\n{src}"));
        }
        count += 1;
    }
    assert_eq!(count, 5);
}

#[test]
fn comments_and_flags() {
    let s = parse(
        "<!-- a surface -->\n<Document>\n  <!-- page -->\n  <Page><Frame clip visible=false /></Page>\n</Document>",
        BTreeMap::new(),
    )
    .unwrap();
    let f = s.node(s.node(s.pages()[0]).unwrap().children[0]).unwrap();
    assert!(f.clips_content);
    assert!(!f.visible);
}

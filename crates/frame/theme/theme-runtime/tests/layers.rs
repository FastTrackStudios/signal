//! A theme in layers: a base, and files that each change one piece.

use kurbo::{Affine, BezPath, Rect, Shape, Stroke};
use peniko::{Brush, Fill};
use theme_runtime::{Canvas, Fields, Schema, Theme, Value};

const BASE: &str = r##"<Document>
  <Variables modes="dark">
    <Var name="off" value="#303030" />
    <Var name="mute" value="#d04040" />
  </Variables>
  <Page name="controls">
    <Component name="button" w=20 h=20 arg:on="false">
      <Rectangle w=20 h=20 bind:fill="arg.on ? $mute : $off" />
    </Component>
    <Component name="lamp" w=10 h=10>
      <Rectangle w=10 h=10 fill="#00ff00" />
    </Component>
  </Page>
  <Page name="mcp.strip">
    <Frame name="Strip" w=40 h=100>
      <Instance name="@mute" of="button" x=10 y=10 arg:on="track.mute" />
      <Instance name="Lamp" of="lamp" x=10 y=40 />
    </Frame>
  </Page>
</Document>
"##;

/// Redraws the button alone: round, and lit another colour.
const BUTTON: &str = r##"<Document>
  <Variables modes="dark">
    <Var name="mute" value="#ff8800" />
  </Variables>
  <Page name="my controls">
    <Component name="button" w=20 h=20 arg:on="false">
      <Ellipse w=20 h=20 bind:fill="arg.on ? $mute : $off" />
    </Component>
  </Page>
</Document>
"##;

/// A different strip, the same controls.
const STRIP: &str = r##"<Document>
  <Page name="mcp.strip">
    <Frame name="Strip" w=40 h=100>
      <Instance name="@mute" of="button" x=10 y=70 arg:on="track.mute" />
    </Frame>
  </Page>
</Document>
"##;

fn schema() -> Schema {
    Schema::new().field("track.mute")
}

#[derive(Default)]
struct Fills(Vec<(String, Rect)>);

impl Canvas for Fills {
    fn fill(
        &mut self,
        transform: Affine,
        _: Fill,
        brush: &Brush,
        _: Option<Affine>,
        shape: &BezPath,
    ) {
        if let Brush::Solid(c) = brush {
            let [r, g, bl, _] = c.to_rgba8().to_u8_array();
            // To the pixel: an ellipse's bounds come out a hair short.
            let b = (transform * shape.clone()).bounding_box();
            self.0.push((
                format!("#{r:02x}{g:02x}{bl:02x}"),
                Rect::new(b.x0.round(), b.y0.round(), b.x1.round(), b.y1.round()),
            ));
        }
    }
    fn stroke(&mut self, _: Affine, _: &Stroke, _: &Brush, _: &BezPath) {}
    fn push_clip(&mut self, _: Affine, _: &BezPath) {}
    fn push_opacity(&mut self, _: Affine, _: f32, _: &BezPath) {}
    fn pop(&mut self) {}
}

fn theme(layers: &[&str]) -> Theme {
    let sources: Vec<(&str, &str)> = layers.iter().map(|s| ("layer", *s)).collect();
    Theme::from_sources(&sources, schema(), None).unwrap_or_else(|d| panic!("{d:?}"))
}

fn drawn(t: &Theme, mute: bool) -> Vec<(String, Rect)> {
    let mut c = Fills::default();
    let f: &dyn Fields = &vec![Value::Bool(mute)];
    t.record("mcp.strip", (40.0, 100.0), f, &mut c, Affine::IDENTITY);
    c.0
}

#[test]
fn one_layer_is_the_theme_itself() {
    let t = theme(&[BASE]);
    assert_eq!(t.roles().collect::<Vec<_>>(), ["mcp.strip"]);
    let d = drawn(&t, true);
    assert!(
        d.contains(&("#d04040".into(), Rect::new(10.0, 10.0, 30.0, 30.0))),
        "{d:?}"
    );
    assert!(d.iter().any(|(h, _)| h == "#00ff00"));
}

#[test]
fn a_component_layered_over_changes_every_place_it_is_used_and_nothing_else() {
    let t = theme(&[BASE, BUTTON]);
    let d = drawn(&t, true);
    // The new button's own token, where the base strip put it.
    assert!(
        d.contains(&("#ff8800".into(), Rect::new(10.0, 10.0, 30.0, 30.0))),
        "{d:?}"
    );
    assert!(!d.iter().any(|(h, _)| h == "#d04040"));
    // The lamp is still the base's.
    assert!(d.iter().any(|(h, _)| h == "#00ff00"));
    // A token it did not set is the base's.
    assert!(drawn(&t, false).iter().any(|(h, _)| h == "#303030"));
}

#[test]
fn a_page_layered_over_replaces_that_role_and_keeps_the_controls() {
    let t = theme(&[BASE, STRIP]);
    let d = drawn(&t, true);
    // The strip is the new one (the button lower, no lamp), drawn with the
    // base's button.
    assert_eq!(
        d,
        [("#d04040".to_owned(), Rect::new(10.0, 70.0, 30.0, 90.0))]
    );
    // And layers stack: the new strip with the new button.
    let d = drawn(&theme(&[BASE, STRIP, BUTTON]), true);
    assert_eq!(
        d,
        [("#ff8800".to_owned(), Rect::new(10.0, 70.0, 30.0, 90.0))]
    );
}

#[test]
fn a_parse_error_names_its_layer() {
    let e = Theme::from_sources(
        &[("base.frame", BASE), ("broken.frame", "<Document><Frame")],
        schema(),
        None,
    )
    .unwrap_err();
    assert!(
        e.iter().any(|d| d.message.starts_with("broken.frame:")),
        "{e:?}"
    );
}

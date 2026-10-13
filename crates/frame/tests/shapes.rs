//! Bindings that reshape a node: one axis of scale, one corner's radius.

use frame::theme::runtime::{Canvas, Fields, Schema, Theme, Value};
use kurbo::{Affine, BezPath, Rect, Shape, Stroke};
use peniko::{Brush, Fill};

const THEME: &str = r##"<Document>
  <Page name="row">
    <Frame name="Row" w=100 h=40>
      <Frame name="Squashed" x=10 y=10 w=20 h=20 bind:scale-y="v">
        <Rectangle w=20 h=20 fill="#ff0000" />
      </Frame>
      <Rectangle name="Pill" x=50 y=10 w=20 h=20 fill="#00ff00" bind:radius-tl="v * 10" bind:radius-bl="v * 10" />
    </Frame>
  </Page>
</Document>
"##;

#[derive(Default)]
struct Fills(Vec<(String, Rect, f64)>);

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
            let [r, g, b, _] = c.to_rgba8().to_u8_array();
            let path = transform * shape.clone();
            self.0.push((
                format!("#{r:02x}{g:02x}{b:02x}"),
                path.bounding_box(),
                path.area().abs(),
            ));
        }
    }
    fn stroke(&mut self, _: Affine, _: &Stroke, _: &Brush, _: &BezPath) {}
    fn push_clip(&mut self, _: Affine, _: &BezPath) {}
    fn push_opacity(&mut self, _: Affine, _: f32, _: &BezPath) {}
    fn pop(&mut self) {}
}

fn drawn(v: f64) -> Vec<(String, Rect, f64)> {
    let t = Theme::from_markup(THEME, Schema::new().field("v"), None)
        .unwrap_or_else(|d| panic!("{d:?}"));
    let f: &dyn Fields = &vec![Value::Num(v)];
    let mut c = Fills::default();
    t.record("row", (100.0, 40.0), f, &mut c, Affine::IDENTITY);
    c.0
}

#[test]
fn scale_y_squashes_about_the_origin_and_keeps_the_width() {
    let d = drawn(0.5);
    let (_, r, _) = d.iter().find(|(h, ..)| h == "#ff0000").expect("red");
    assert_eq!(*r, Rect::new(10.0, 10.0, 30.0, 20.0));
}

#[test]
fn one_corner_radius_rounds_that_corner_only() {
    let square = 400.0;
    let quarter = 100.0 - std::f64::consts::PI * 25.0;
    let area = |v| {
        drawn(v)
            .into_iter()
            .find(|(h, ..)| h == "#00ff00")
            .map(|(.., a)| a)
            .expect("green")
    };
    assert!((area(0.0) - square).abs() < 0.5);
    // Rounded on its left end only: two corners' worth taken off.
    assert!(
        (area(0.5) - (square - 2.0 * quarter * 0.25)).abs() < 0.5,
        "{}",
        area(0.5)
    );
    assert!(
        (area(1.0) - (square - 2.0 * quarter)).abs() < 0.5,
        "{}",
        area(1.0)
    );
}

/// A target sized by host data is hit and reported at that size.
#[test]
fn a_target_sized_by_host_data_is_that_size() {
    let src = r##"<Document><Page name="row"><Frame name="Row" w=100 h=40>
      <Frame name="@name" x=10 y=5 w=10 h=10 bind:w="v * 50" />
    </Frame></Page></Document>"##;
    let t =
        Theme::from_markup(src, Schema::new().field("v"), None).unwrap_or_else(|d| panic!("{d:?}"));
    let f: &dyn Fields = &vec![Value::Num(1.0)];
    assert_eq!(
        t.target_rect("row", (100.0, 40.0), f, "name"),
        Some(Rect::new(10.0, 5.0, 60.0, 15.0))
    );
    assert_eq!(
        t.hit("row", (100.0, 40.0), f, kurbo::Point::new(55.0, 10.0))
            .as_deref(),
        Some("name")
    );
}

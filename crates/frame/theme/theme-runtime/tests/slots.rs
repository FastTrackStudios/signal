//! Host slots: places the theme lays out for what the host draws itself.

use kurbo::Rect;
use theme_runtime::{Fields, Schema, Theme, Value};

const THEME: &str = r##"<Document>
  <Page name="parts">
    <Component name="button" w=40 h=40 arg:icon="true">
      <Rectangle name="Plate" w=40 h=40 fill="#303030" />
      <Frame name="%icon" x=4 y=4 w=32 h=32 bind:visible="arg.icon" />
    </Component>
  </Page>
  <Page name="rail">
    <Frame name="Rail" w=44 h=200>
      <Frame name="%under" w=44 h=10 />
      <Instance name="@first" of="button" x=2 y=20 arg:icon="has.icon" />
      <Frame name="%graph" x=2 y=100 w=40 h=60 bind:y="h - 70" />
    </Frame>
  </Page>
</Document>
"##;

fn theme() -> Theme {
    Theme::from_markup(THEME, Schema::new().field("has.icon"), None)
        .unwrap_or_else(|d| panic!("{d:?}"))
}

fn slots(t: &Theme, size: (f64, f64), icon: bool) -> Vec<(String, Rect)> {
    let f: &dyn Fields = &vec![Value::Bool(icon)];
    t.slots("rail", size, f)
}

#[test]
fn slots_are_where_the_theme_puts_them_in_paint_order() {
    let t = theme();
    let s = slots(&t, (44.0, 200.0), true);
    assert_eq!(
        s,
        [
            ("under".to_owned(), Rect::new(0.0, 0.0, 44.0, 10.0)),
            // Inside a placed component, where its instance put it.
            ("icon".to_owned(), Rect::new(6.0, 24.0, 38.0, 56.0)),
            ("graph".to_owned(), Rect::new(2.0, 130.0, 42.0, 190.0)),
        ]
    );
    // Moved by its binding at another size.
    let tall = slots(&t, (44.0, 400.0), true);
    assert_eq!(
        tall.last().map(|(_, r)| *r),
        Some(Rect::new(2.0, 330.0, 42.0, 390.0))
    );
}

#[test]
fn a_hidden_slot_is_not_offered() {
    let t = theme();
    let names: Vec<String> = slots(&t, (44.0, 200.0), false)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert_eq!(names, ["under", "graph"]);
    // And a slot is not a hit target.
    let f: &dyn Fields = &vec![Value::Bool(true)];
    assert_eq!(t.targets("rail", (44.0, 200.0), f), ["first"]);
}

const PANEL: &str = r##"<Document>
  <Page name="panel">
    <Frame name="Panel" w=100 h=60>
      <Rectangle name="Ground" w=100 h=60 bind:w="w" fill="#101010" />
      <Frame name="%body" x=4 y=16 w=92 h=40 bind:w="w - 8" />
      <Rectangle name="Scrim" w=100 h=60 bind:w="w" fill="#202020" bind:visible="has.icon" />
      <Text name="Title" x=4 y=2 w=92 h=12 size=10 fill="#e0e0e0">EQ</Text>
      <Frame name="%hidden" w=10 h=10 bind:visible="!has.icon" />
    </Frame>
  </Page>
</Document>
"##;

/// Records the colour of each fill, and what the host drew where.
#[derive(Default)]
struct Order(Vec<String>);

impl theme_runtime::Canvas for Order {
    fn fill(
        &mut self,
        _: kurbo::Affine,
        _: peniko::Fill,
        brush: &peniko::Brush,
        _: Option<kurbo::Affine>,
        _: &kurbo::BezPath,
    ) {
        if let peniko::Brush::Solid(c) = brush {
            let [r, g, b, _] = c.to_rgba8().to_u8_array();
            self.0.push(format!("#{r:02x}{g:02x}{b:02x}"));
        }
    }
    fn stroke(
        &mut self,
        _: kurbo::Affine,
        _: &kurbo::Stroke,
        _: &peniko::Brush,
        _: &kurbo::BezPath,
    ) {
    }
    fn push_clip(&mut self, _: kurbo::Affine, _: &kurbo::BezPath) {}
    fn push_opacity(&mut self, _: kurbo::Affine, _: f32, _: &kurbo::BezPath) {}
    fn pop(&mut self) {}
}

#[test]
fn the_host_draws_its_slot_between_what_the_theme_paints_under_and_over_it() {
    let t = Theme::from_markup(PANEL, Schema::new().field("has.icon"), None)
        .unwrap_or_else(|d| panic!("{d:?}"));
    let f: &dyn Fields = &vec![Value::Bool(true)];
    let mut canvas = Order::default();
    t.record_with(
        "panel",
        (200.0, 60.0),
        f,
        &mut canvas,
        kurbo::Affine::IDENTITY,
        &mut |c, name, at| {
            c.0.push(format!("{name} {} {} {} {}", at.x0, at.y0, at.x1, at.y1));
        },
    );
    // Ground under the body, the scrim and the title over it; the hidden
    // slot is not offered.
    assert_eq!(
        canvas.0,
        ["#101010", "body 4 16 196 56", "#202020", "#e0e0e0"]
    );

    // The plain recording paints the theme alone.
    let mut plain = Order::default();
    t.record(
        "panel",
        (200.0, 60.0),
        f,
        &mut plain,
        kurbo::Affine::IDENTITY,
    );
    assert_eq!(plain.0, ["#101010", "#202020", "#e0e0e0"]);
}

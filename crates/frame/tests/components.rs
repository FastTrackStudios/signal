//! Components: a control drawn once, placed many times with its own
//! arguments.

use frame::theme::runtime::{Canvas, Fields, Schema, Theme, Value};
use kurbo::{Affine, BezPath, Point, Rect, Shape, Stroke};
use peniko::{Brush, Fill};

const THEME: &str = r##"<Document>
  <Variables modes="dark">
    <Var name="off" value="#303030" />
    <Var name="mute" value="#d04040" />
    <Var name="solo" value="#d0c040" />
    <Var name="hover" value="#505050" />
  </Variables>
  <Page name="controls">
    <Component name="button" w=20 h=20 arg:on="false" arg:lit="$mute" arg:control="\"\"">
      <Rectangle name="Face" w=20 h=20 bind:fill="arg.on ? arg.lit : (ui.hover == arg.control ? $hover : $off)" />
    </Component>
    <Component name="pair" w=20 h=44 arg:muted="false">
      <Instance name="@mute" of="button" w=20 h=20 arg:on="arg.muted" arg:control="\"mute\"" />
      <Instance name="@solo" of="button" y=24 w=20 h=20 arg:on="track.solo" arg:lit="$solo" arg:control="\"solo\"" />
    </Component>
  </Page>
  <Page name="mcp.strip">
    <Frame name="Strip" w=40 h=100>
      <Instance name="Pair" of="pair" x=10 y=10 arg:muted="track.mute" bind:visible="w >= 30" />
    </Frame>
  </Page>
</Document>
"##;

fn schema() -> Schema {
    Schema::new()
        .field("track.mute")
        .field("track.solo")
        .live("ui.hover")
}

fn theme(src: &str) -> Result<Theme, Vec<String>> {
    Theme::from_markup(src, schema(), None).map_err(|d| d.iter().map(ToString::to_string).collect())
}

fn fields(mute: bool, solo: bool, hover: &str) -> Vec<Value> {
    vec![Value::Bool(mute), Value::Bool(solo), Value::text(hover)]
}

/// Collects what was filled: each solid colour and where.
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
            let [r, g, b, _] = c.to_rgba8().to_u8_array();
            self.0.push((
                format!("#{r:02x}{g:02x}{b:02x}"),
                (transform * shape.clone()).bounding_box(),
            ));
        }
    }
    fn stroke(&mut self, _: Affine, _: &Stroke, _: &Brush, _: &BezPath) {}
    fn push_clip(&mut self, _: Affine, _: &BezPath) {}
    fn push_opacity(&mut self, _: Affine, _: f32, _: &BezPath) {}
    fn pop(&mut self) {}
}

/// Everything drawn, recorded and live, by colour.
fn drawn(t: &Theme, size: (f64, f64), f: &dyn Fields) -> Vec<(String, Rect)> {
    let mut c = Fills::default();
    t.record("mcp.strip", size, f, &mut c, Affine::IDENTITY);
    t.live("mcp.strip", size, f, &mut c, Affine::IDENTITY);
    c.0
}

#[test]
fn a_component_page_is_not_a_role() {
    let t = theme(THEME).unwrap();
    assert_eq!(t.roles().collect::<Vec<_>>(), ["mcp.strip"]);
}

#[test]
fn each_instance_draws_its_component_with_its_own_arguments() {
    let t = theme(THEME).unwrap();
    let at = |hex: &str, f: &Vec<Value>| {
        drawn(&t, (40.0, 100.0), f)
            .into_iter()
            .filter(|(h, _)| h == hex)
            .map(|(_, r)| r)
            .collect::<Vec<_>>()
    };
    // Nothing on: both buttons are the off face, where their instances put them.
    let off = at("#303030", &fields(false, false, ""));
    assert_eq!(
        off,
        [
            Rect::new(10.0, 10.0, 30.0, 30.0),
            Rect::new(10.0, 34.0, 30.0, 54.0)
        ]
    );
    // Muted: the argument passed down two levels lights the first only.
    assert_eq!(
        at("#d04040", &fields(true, false, "")),
        [Rect::new(10.0, 10.0, 30.0, 30.0)]
    );
    // Soloed: the second's own argument, its own colour.
    assert_eq!(
        at("#d0c040", &fields(false, true, "")),
        [Rect::new(10.0, 34.0, 30.0, 54.0)]
    );
    // Hover is per control: each compares against its own name.
    assert_eq!(
        at("#505050", &fields(false, false, "solo")),
        [Rect::new(10.0, 34.0, 30.0, 54.0)]
    );
}

#[test]
fn an_instance_keeps_its_name_and_its_own_bindings() {
    let t = theme(THEME).unwrap();
    let f = fields(false, false, "");
    // The instances' names are the hit targets.
    assert_eq!(
        t.hit("mcp.strip", (40.0, 100.0), &f, Point::new(15.0, 15.0))
            .as_deref(),
        Some("mute")
    );
    assert_eq!(
        t.hit("mcp.strip", (40.0, 100.0), &f, Point::new(15.0, 40.0))
            .as_deref(),
        Some("solo")
    );
    // The instance's own `bind:visible` hides the copy.
    assert!(
        drawn(&t, (20.0, 100.0), &f)
            .iter()
            .all(|(h, _)| h != "#303030")
    );
}

#[test]
fn problems_with_components_are_reported() {
    let unknown = THEME.replace(r#"of="pair""#, r#"of="pear""#);
    let e = theme(&unknown).unwrap_err();
    assert!(
        e.iter()
            .any(|e| e.contains("no component `pear`") && e.contains("button, pair")),
        "{e:?}"
    );

    let extra = THEME.replace(
        r#"arg:muted="track.mute""#,
        r#"arg:muted="track.mute" arg:colour="$mute""#,
    );
    let e = theme(&extra).unwrap_err();
    assert!(
        e.iter()
            .any(|e| e.contains("`pair` takes no argument `colour`")
                && e.contains("it takes: muted")),
        "{e:?}"
    );

    let unread = THEME.replace("arg.lit", "arg.light");
    let e = theme(&unread).unwrap_err();
    assert!(e.iter().any(|e| e.contains("no argument `light`")), "{e:?}");

    let outside = THEME.replace(r#"bind:visible="w >= 30""#, r#"bind:visible="arg.muted""#);
    let e = theme(&outside).unwrap_err();
    assert!(
        e.iter()
            .any(|e| e.contains("`arg.muted` is read outside a component")),
        "{e:?}"
    );

    let recursive = THEME.replace(
        r#"<Instance name="@solo" of="button""#,
        r#"<Instance name="@solo" of="pair""#,
    );
    let e = theme(&recursive).unwrap_err();
    assert!(
        e.iter().any(|e| e.contains("does `pair` place itself?")),
        "{e:?}"
    );
}

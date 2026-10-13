//! A mixer strip drawn from a theme, as a host uses it.

use kurbo::{Affine, BezPath, Point, Rect, Shape, Stroke};
use peniko::{Brush, Fill};
use theme_runtime::{Canvas, Fields, Rgba, Schema, Theme, Value};

const STRIP: &str = r##"<Document>
  <Variables modes="dark light">
    <Var name="strip.ground" dark="#202020" light="#e8e8e8" />
    <Var name="button" value="#3a3a3a" />
    <Var name="mute.on" value="#d04040" />
    <Var name="meter" value="#40d070" />
    <Var name="cap" value="mix($button, #ffffff, 0.5)" />
  </Variables>
  <Page name="mcp.strip">
    <Frame name="Strip" w=86 h=300 bind:fill="mix($strip.ground, track.color, 0.25)">
      <Frame name="Buttons" x=4 y=4 w=40 h=80 layout="column" gap=2>
        <Frame name="@mute" w=40 h=18 bind:fill="track.mute ? $mute.on : $button" />
        <Frame name="@solo" w=40 h=18 fill="#3a3a3a" bind:visible="h >= 250" />
        <Frame name="@arm" w=40 h=18 fill="#3a3a3a" />
      </Frame>
      <Frame name="Travel" x=50 y=20 w=20 h=240 resize-y="stretch">
        <Frame name="@fader" w=20 h=12 bind:fill="$cap" bind:y="(1 - track.fader) * (ph - 12)" />
      </Frame>
      <Frame name="Well" x=74 y=20 w=8 h=240 resize-y="stretch" fill="#101010">
        <Rectangle name="Level" w=8 h=240 bind:fill="$meter" bind:h="ph * meter.level" bind:y="ph * (1 - meter.level)" />
      </Frame>
      <Text name="@name" x=2 y=280 w=82 h=16 resize-y="bottom" resize-x="stretch" size=11 bind:text="track.name" bind:fill="ink($strip.ground)" bind:visible="w >= 40">Track</Text>
    </Frame>
  </Page>
</Document>
"##;

fn schema() -> Schema {
    Schema::new().field("track.name").field("track.color").field("track.mute").live("track.fader").live("meter.level")
}

fn theme() -> Theme {
    Theme::from_markup(STRIP, schema(), None).unwrap_or_else(|d| panic!("{d:?}"))
}

/// A track, in schema order.
fn track(name: &str, mute: bool, fader: f64, level: f64) -> Vec<Value> {
    vec![
        Value::text(name),
        Value::Color(Rgba::from_hex("#3366cc").unwrap()),
        Value::Bool(mute),
        Value::Num(fader),
        Value::Num(level),
    ]
}

/// Collects what was filled: each solid colour and the shape's bounds.
#[derive(Default)]
struct Fills(Vec<(String, Rect)>);

impl Canvas for Fills {
    fn fill(&mut self, transform: Affine, _: Fill, brush: &Brush, _: Option<Affine>, shape: &BezPath) {
        if let Brush::Solid(c) = brush {
            let [r, g, b, a] = c.to_rgba8().to_u8_array();
            let hex = if a == 255 { format!("#{r:02x}{g:02x}{b:02x}") } else { format!("#{r:02x}{g:02x}{b:02x}{a:02x}") };
            self.0.push((hex, (transform * shape.clone()).bounding_box()));
        }
    }
    fn stroke(&mut self, _: Affine, _: &Stroke, _: &Brush, _: &BezPath) {}
    fn push_clip(&mut self, _: Affine, _: &BezPath) {}
    fn push_opacity(&mut self, _: Affine, _: f32, _: &BezPath) {}
    fn pop(&mut self) {}
}

impl Fills {
    fn has(&self, hex: &str) -> bool {
        self.0.iter().any(|(h, _)| h == hex)
    }

    fn rect(&self, hex: &str) -> Rect {
        self.0.iter().find(|(h, _)| h == hex).map(|(_, r)| *r).unwrap_or_else(|| panic!("no {hex} in {:?}", self.0))
    }
}

fn record(theme: &Theme, size: (f64, f64), fields: &dyn Fields) -> Fills {
    let mut c = Fills::default();
    assert!(theme.record("mcp.strip", size, fields, &mut c, Affine::IDENTITY));
    c
}

fn live(theme: &Theme, size: (f64, f64), fields: &dyn Fields) -> Fills {
    let mut c = Fills::default();
    assert!(theme.live("mcp.strip", size, fields, &mut c, Affine::IDENTITY));
    c
}

#[test]
fn it_compiles_with_its_roles_and_targets() {
    let t = theme();
    assert_eq!(t.roles().collect::<Vec<_>>(), ["mcp.strip"]);
    assert_eq!(t.authored_size("mcp.strip"), Some((86.0, 300.0)));
    assert!(t.has_live("mcp.strip"));
    // Topmost first.
    assert_eq!(t.targets("mcp.strip", (86.0, 300.0), &track("Vox", false, 0.0, 0.0)), ["name", "fader", "arm", "solo", "mute"]);
}

#[test]
fn static_data_is_recorded_and_live_data_is_not() {
    let t = theme();
    let muted = record(&t, (86.0, 300.0), &track("Vox", true, 0.5, 0.5));
    let open = record(&t, (86.0, 300.0), &track("Vox", false, 0.5, 0.5));
    // The mute button reads static data: it is in the recording, lit or not.
    assert!(muted.has("#d04040") && !open.has("#d04040"));
    assert!(open.has("#3a3a3a"));
    // The strip's ground is tinted by the track's colour.
    assert!(open.has("#25324b"), "{:?}", open.0);
    // The cap and the meter read live data: not recorded.
    assert!(!open.has("#9d9d9d") && !open.has("#40d070"));
    let l = live(&t, (86.0, 300.0), &track("Vox", false, 0.5, 0.5));
    assert!(l.has("#9d9d9d") && l.has("#40d070"));
    // And the live pass draws only the live parts.
    assert!(!l.has("#3a3a3a"));
}

#[test]
fn live_bindings_move_and_size_their_nodes() {
    let t = theme();
    let size = (86.0, 300.0);
    // Fader at the top of its travel, at the bottom, half way.
    let cap = |fader: f64| live(&t, size, &track("Vox", false, fader, 0.0)).rect("#9d9d9d");
    assert_eq!(cap(1.0).y0, 20.0);
    assert_eq!(cap(0.0).y0, 20.0 + 240.0 - 12.0);
    assert_eq!(cap(0.5).y0, 20.0 + 114.0);
    // The meter rises from the floor of its well.
    let level = live(&t, size, &track("Vox", false, 0.0, 0.25)).rect("#40d070");
    assert_eq!((level.y0, level.y1), (20.0 + 180.0, 260.0));
}

#[test]
fn a_taller_strip_stretches_the_travel_and_drops_the_name() {
    let t = theme();
    let size = (86.0, 500.0);
    let cap = live(&t, size, &track("Vox", false, 0.0, 0.0)).rect("#9d9d9d");
    // Travel 240 + 200: the cap at the bottom is 200 lower.
    assert_eq!(cap.y0, 20.0 + 440.0 - 12.0);
    let name = t.target_rect("mcp.strip", size, &track("Vox", false, 0.0, 0.0), "name").unwrap();
    assert_eq!(name.y0, 480.0);
}

#[test]
fn a_narrow_strip_hides_what_the_theme_says() {
    let t = theme();
    let fields = track("Vox", false, 0.0, 0.0);
    assert!(t.target_rect("mcp.strip", (86.0, 300.0), &fields, "name").is_some());
    assert!(t.target_rect("mcp.strip", (30.0, 300.0), &fields, "name").is_none());
}

#[test]
fn a_hidden_row_leaves_the_flow() {
    let t = theme();
    let fields = track("Vox", false, 0.0, 0.0);
    let arm = |h: f64| t.target_rect("mcp.strip", (86.0, h), &fields, "arm").unwrap();
    // Tall: mute, solo, arm.
    assert_eq!(arm(300.0).y0, 4.0 + 18.0 + 2.0 + 18.0 + 2.0);
    // Short: solo is hidden and arm closes up under mute.
    assert!(t.target_rect("mcp.strip", (86.0, 200.0), &fields, "solo").is_none());
    assert_eq!(arm(200.0).y0, 4.0 + 18.0 + 2.0);
}

#[test]
fn the_pointer_finds_the_control_where_it_is_drawn() {
    let t = theme();
    let size = (86.0, 300.0);
    let at = |fader: f64, p: (f64, f64)| t.hit("mcp.strip", size, &track("Vox", false, fader, 0.0), Point::new(p.0, p.1));
    assert_eq!(at(0.0, (10.0, 10.0)).as_deref(), Some("mute"));
    // The fader cap is where its value put it.
    assert_eq!(at(1.0, (60.0, 25.0)).as_deref(), Some("fader"));
    assert_eq!(at(0.0, (60.0, 25.0)), None);
    assert_eq!(at(0.0, (60.0, 255.0)).as_deref(), Some("fader"));
}

#[test]
fn modes_swap_the_tokens() {
    let dark = theme();
    let light = dark.with_mode("light").unwrap();
    assert_eq!(light.mode(), "light");
    assert_eq!(dark.token("strip.ground").and_then(Value::color).unwrap().to_hex(), "#202020");
    assert_eq!(light.token("strip.ground").and_then(Value::color).unwrap().to_hex(), "#e8e8e8");
    // Derived tokens follow.
    assert_eq!(dark.token("cap").and_then(Value::color).unwrap().to_hex(), "#9d9d9d");
    let fields = track("Vox", false, 0.0, 0.0);
    assert!(record(&light, (86.0, 300.0), &fields).has("#bbc8e1"));
}

#[test]
fn problems_are_reported_by_where_they_are() {
    let bad = STRIP.replace("track.name", "track.nom").replace("$meter", "$metre");
    let errs = Theme::from_markup(&bad, schema(), None).unwrap_err();
    let text: Vec<String> = errs.iter().map(ToString::to_string).collect();
    assert!(text.iter().any(|e| e.contains("mcp.strip › @name") && e.contains("no field `track.nom`")), "{text:?}");
    assert!(text.iter().any(|e| e.contains("Level") && e.contains("no token `$metre`")), "{text:?}");
    let cyclic = STRIP.replace(r##"value="#3a3a3a""##, r##"value="$cap""##);
    let errs = Theme::from_markup(&cyclic, schema(), None).unwrap_err();
    assert!(errs.iter().any(|e| e.message.contains("refers to itself")), "{errs:?}");
}

#[test]
fn a_bound_rotation_turns_the_node_about_its_origin() {
    let src = r##"<Document>
  <Page name="knob">
    <Frame name="Knob" w=24 h=24>
      <Vector name="Pointer" x=12 y=12 w=1 h=1 fill="#ffffff" geometry="M -1 0 L 1 0 L 1 -10 L -1 -10 Z" bind:rotation="track.pan * 90" />
    </Frame>
  </Page>
</Document>"##;
    let t = Theme::from_markup(src, Schema::new().live("track.pan"), None).unwrap();
    let pointer = |pan: f64| {
        let mut c = Fills::default();
        t.live("knob", (24.0, 24.0), &vec![Value::Num(pan)], &mut c, Affine::IDENTITY);
        c.rect("#ffffff")
    };
    // Centre: straight up from the middle.
    let up = pointer(0.0);
    assert!((up.y0 - 2.0).abs() < 1e-6 && (up.x0 - 11.0).abs() < 1e-6, "{up:?}");
    // Hard right: lying along the right-hand radius.
    let right = pointer(1.0);
    assert!((right.x1 - 22.0).abs() < 1e-6 && (right.y0 - 11.0).abs() < 1e-6, "{right:?}");
}

#[test]
fn an_inset_reveals_a_fixed_picture_and_a_dash_draws_part_of_a_stroke() {
    let src = r##"<Document>
  <Page name="meter">
    <Frame name="Meter" w=10 h=100>
      <Frame name="Lit" w=10 h=100 clip=true bind:inset-top="100 * (1 - meter.level)">
        <Rectangle name="Ramp" w=10 h=100 fill="#00ff00" />
      </Frame>
      <Vector name="Arc" x=0 y=0 w=10 h=10 stroke="#ff0000" stroke-weight=2 geometry="M 0 0 L 10 0" bind:dash="10 * meter.level" />
      <Text name="Label" y=90 w=10 h=10 bind:size="4 + meter.level * 4" fill="#0000ff">x</Text>
    </Frame>
  </Page>
</Document>"##;
    let t = Theme::from_markup(src, Schema::new().live("meter.level"), None).unwrap();

    /// What was clipped to, and what the stroke dash was.
    #[derive(Default)]
    struct Probe {
        clips: Vec<Rect>,
        dashes: Vec<Vec<f64>>,
        sizes: Vec<f64>,
    }
    impl Canvas for Probe {
        fn fill(&mut self, _: Affine, _: Fill, _: &Brush, _: Option<Affine>, _: &BezPath) {}
        fn stroke(&mut self, _: Affine, style: &Stroke, _: &Brush, _: &BezPath) {
            self.dashes.push(style.dash_pattern.to_vec());
        }
        fn push_clip(&mut self, t: Affine, shape: &BezPath) {
            self.clips.push((t * shape.clone()).bounding_box());
        }
        fn push_opacity(&mut self, _: Affine, _: f32, _: &BezPath) {}
        fn pop(&mut self) {}
        fn handles_text(&self) -> bool {
            true
        }
        fn text(&mut self, _: Affine, run: &theme_runtime::TextRun) -> bool {
            self.sizes.push(run.size);
            true
        }
    }
    let mut p = Probe::default();
    t.live("meter", (10.0, 100.0), &vec![Value::Num(0.25)], &mut p, Affine::IDENTITY);
    let clip = p.clips.first().copied().expect("clipped");
    // A quarter lit: the clip is the bottom quarter, the ramp unmoved.
    assert_eq!((clip.y0, clip.y1), (75.0, 100.0));
    assert_eq!(p.dashes.first().and_then(|d| d.first()).copied(), Some(2.5));
    // The host drew the text, at the bound size.
    assert_eq!(p.sizes, [5.0]);
}

#[test]
fn a_layout_field_shapes_the_layout_like_the_box_does() {
    let src = r##"<Document>
  <Page name="strip">
    <Frame name="Strip" w=86 h=300>
      <Frame name="@pan" y=40 w=20 h=20 bind:visible="mixer.h >= 250" />
    </Frame>
  </Page>
</Document>"##;
    let t = Theme::from_markup(src, Schema::new().layout("mixer.h"), None).unwrap();
    let pan = |mixer_h: f64| t.target_rect("strip", (86.0, 200.0), &vec![Value::Num(mixer_h)], "pan");
    // The same box, laid out against two mixer heights.
    assert!(pan(300.0).is_some());
    assert!(pan(200.0).is_none());
    // A layout binding is not a per-instance one: nothing reads it live.
    assert!(!t.has_live("strip"));
}

#[test]
fn a_static_node_over_a_live_one_stays_over_it() {
    // The face lights on hover (live); the cap over it is static. Painted
    // in two passes, the cap would end up under the face.
    let src = r##"<Document>
  <Page name="knob">
    <Frame name="Knob" w=24 h=24>
      <Ellipse name="Face" x=2 y=2 w=20 h=20 bind:fill="ui.hover ? #ff0000 : #00ff00" />
      <Ellipse name="Cap" x=8 y=8 w=8 h=8 fill="#0000ff" />
      <Rectangle name="Elsewhere" x=30 y=0 w=4 h=4 fill="#ffffff" />
    </Frame>
  </Page>
</Document>"##;
    let t = Theme::from_markup(src, Schema::new().live("ui.hover"), None).unwrap();
    let fields = vec![Value::Bool(false)];
    let (mut rec, mut live) = (Fills::default(), Fills::default());
    t.record("knob", (40.0, 24.0), &fields, &mut rec, Affine::IDENTITY);
    t.live("knob", (40.0, 24.0), &fields, &mut live, Affine::IDENTITY);
    // The cap follows the face into the live pass, after it.
    let order: Vec<&str> = live.0.iter().map(|(h, _)| h.as_str()).collect();
    assert_eq!(order, ["#00ff00", "#0000ff"]);
    // What does not overlap stays recorded.
    assert!(rec.has("#ffffff") && !rec.has("#0000ff"));
}

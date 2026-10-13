//! The pitch unit: four voices, and a ladder that shows
//! where every voice sits against the note.
//!
//! The ladder runs two octaves down to two up, its octaves and unison
//! marked: the dry note a white line at unison, each voice a bar in its
//! colour at its interval — as long as its level. Beside it, a column
//! per voice (its interval, read as a musical name, and its level), and
//! the dry level and mix. The doubler is a unit of its own (`doubler.rs`).
//!
//! Voices A and B are the rig's pitch block (`semitones`/`a_level`,
//! `b_semitones`/`b_level`); C and D are drawn and bound for the engine
//! to take up (`c_*`, `d_*`).

use document_proto::motion::{Dynamics, Motion, Target};
use document_proto::{CornerRadii, NodeId, ResizeX, TextAlign};
use surface_kit::draw::{self, Lettering, family};
use surface_kit::motion;
use surface_proto::{Range, Unit, ValueFormat};
use surface_widgets::Builder;

use super::PLUGIN;
use super::drives::H;
use crate::face::{Face, canvas};
use crate::reverb::ui;
use crate::rig_time::kit;
use crate::suite_studio_blues::parts::set;

const NS: &str = "pitch";
const INK: &str = "#ece7dc";
const DIM: &str = "#8b8f98";
const W: f64 = 540.0;

/// Each voice: its label, interval and level params, its colour.
const VOICES: [(&str, &str, &str, &str); 4] = [
    ("A", "semitones", "a_level", "#f5b041"),
    ("B", "b_semitones", "b_level", "#5dade2"),
    ("C", "c_semitones", "c_level", "#58d68d"),
    ("D", "d_semitones", "d_level", "#ec7fbf"),
];

fn a(id: &str) -> String {
    format!("{NS}/{id}")
}

/// An interval read as music: octaves, unison and the common harmonies.
fn interval_format() -> ValueFormat {
    [(24.0, "2 Oct ↑"), (19.0, "Oct+5th"), (12.0, "Oct ↑"), (7.0, "5th ↑"), (5.0, "4th ↑"), (4.0, "M3 ↑"), (3.0, "m3 ↑"), (0.0, "Unison"), (-3.0, "m3 ↓"), (-4.0, "M3 ↓"), (-5.0, "4th ↓"), (-7.0, "5th ↓"), (-12.0, "Oct ↓"), (-24.0, "2 Oct ↓")]
        .iter()
        .fold(ValueFormat::decimals(0).signed(), |f, (at, l)| f.mark(*at, *l))
}

/// The ladder in `v` (`w × h`): the scale, the dry note, the voices.
fn ladder(b: &mut Builder<'_>, v: NodeId, w: f64, h: f64) {
    let (top, bottom) = (14.0, h - 14.0);
    let y_of = |semis: f64| top + (bottom - top) * (24.0 - semis) / 48.0;
    // The scale: a line a semitone, the octaves and unison marked.
    let mut minor = Vec::new();
    for s in -24..=24 {
        if s % 12 != 0 {
            let y = y_of(f64::from(s));
            minor.push(vec![(44.0, y), (52.0, y)]);
        }
    }
    b.add(v, ui::strokes("semitones", &minor, 1.0, draw::solid("#ffffff1c")));
    for (s, label) in [(24, "+2 OCT"), (12, "+OCT"), (0, "UNISON"), (-12, "−OCT"), (-24, "−2 OCT")] {
        let y = y_of(f64::from(s));
        let mut line = ui::strokes("octave", &[vec![(44.0, y), (w - 8.0, y)]], if s == 0 { 1.2 } else { 1.0 }, draw::solid(if s == 0 { "#ffffff38" } else { "#ffffff20" }));
        line.layout_child.resize_x = Some(ResizeX::Stretch);
        b.add(v, line);
        b.add(v, ui::text("octave label", label, 2.0, y, 40.0, 6.5, DIM, TextAlign::Right, 800.0, false));
    }
    // The dry note: a white line at unison, as long as the dry level.
    let dry = b.add(v, draw::frame("dry", 56.0, y_of(0.0) - 2.0, w - 70.0, 4.0));
    if let Some(n) = b.scene.node_mut(dry) {
        n.layout_child.resize_x = Some(ResizeX::Stretch);
    }
    b.add(dry, ui::glowing(draw::rounded(draw::rect("note", 0.0, 0.0, w - 70.0, 4.0, draw::solid("#f7f5ef")), 2.0), "#ffffff", 0.6, 4.0));
    motion::add(b, dry, Motion::new(Target::StretchX).input("param(pitch/dry)".to_string()).range(0.02, 1.0).pivot(0.0, 0.5).dynamics(Dynamics::spring(300.0, 30.0)));
    // The voices: a bar each at its interval, as long as its level.
    let bar_h = 10.0;
    let lane_w = w - 70.0;
    for (i, (name, st, lv, color)) in VOICES.iter().enumerate() {
        // Placed at unison, moved to its interval.
        let x0 = 56.0 + 6.0 * i as f64;
        let g = b.add(v, draw::frame(&format!("voice {name}"), x0, y_of(0.0) - bar_h / 2.0, lane_w - 6.0 * i as f64, bar_h));
        if let Some(n) = b.scene.node_mut(g) {
            n.layout_child.resize_x = Some(ResizeX::Stretch);
        }
        let bar = b.add(g, draw::frame("bar", 0.0, 0.0, lane_w - 6.0 * i as f64, bar_h));
        b.add(bar, ui::glowing(draw::rounded(draw::rect("level", 0.0, 0.0, lane_w - 6.0 * i as f64, bar_h, draw::linear(90.0, &[(0.0, color), (1.0, &ui::alpha(color, 0.25))])), bar_h / 2.0), color, 0.7, 6.0));
        b.add(g, ui::text("tag", name, 4.0, bar_h / 2.0, 12.0, 7.0, "#101114", TextAlign::Center, 900.0, false));
        let span = (bottom - top) / 2.0;
        motion::add(b, g, Motion::new(Target::Y).input(format!("clamp(param({NS}/{st}) / 24, -1, 1) * 0.5 + 0.5")).range(span, -span).dynamics(Dynamics::spring(260.0, 26.0)));
        motion::add(b, bar, Motion::new(Target::StretchX).input(format!("param({NS}/{lv})")).range(0.03, 1.0).pivot(0.0, 0.5).dynamics(Dynamics::spring(260.0, 26.0)));
        motion::add(b, g, Motion::new(Target::Opacity).input(format!("0.25 + 0.75 * clamp(param({NS}/{lv}) * 4, 0, 1)")).range(0.0, 1.0));
    }
}

/// A knob with its legend, its value read under it.
#[allow(clippy::too_many_arguments)]
fn dial(b: &mut Builder<'_>, p: NodeId, addr: &str, legend: &str, d: f64, (cx, cy): (f64, f64), pointer: &str, format: Option<ValueFormat>) -> NodeId {
    let ptr = format!("pointer={pointer}");
    let opts = ["color=#1c1d21", ptr.as_str(), "index=line", "index_w=0.1", "skirt=plain", "skirt_color=#0b0b0c"];
    let k = kit::knob3d(b, p, addr, legend, d, (cx, cy), &opts, INK, DIM);
    if let Some(f) = format
        && let Some(c) = b.controls.get_mut(&k)
    {
        c.format = Some(f);
    }
    k
}

/// The unit.
#[must_use]
pub fn pitch_unit() -> Face {
    let scene = canvas("Pitch", |b, root| {
        ui::fit_root(b, root, W, H);
        let mut p = draw::frame("Panel", 0.0, 0.0, W, H);
        p.fills = vec![draw::linear(180.0, &[(0.0, "#1a1b20"), (1.0, "#101115")])];
        let p = b.add(root, draw::with_effects(p, vec![draw::inner_shadow("#ffffff14", 0.0, 1.0, 0.0)]));
        let head = Lettering::family(family::INTER).weight(800.0);
        set(b, p, "PITCH", 36.0, 18.0, 12.0, "#facc15", &head, 0.3);
        ui::silk_left(b, p, "Four voices · harmony", 72.0, 18.0, 6.5, DIM, 700.0);

        // ── The ladder ──────────────────────────────────────────────────
        let (vx, vy, vw, vh) = (12.0, 30.0, 190.0, H - 42.0);
        let mut v = draw::frame("Ladder", vx, vy, vw, vh);
        v.fills = vec![draw::linear(180.0, &[(0.0, "#0c0d10"), (1.0, "#08090b")])];
        v.clips_content = true;
        v.corner_radii = CornerRadii::uniform(6.0);
        v.effects = vec![draw::inner_shadow("#000000c0", 0.0, 2.0, 6.0)];
        let v = b.add(p, v);
        ladder(b, v, vw, vh);

        // ── The voices ──────────────────────────────────────────────────
        let x0 = vx + vw + 18.0;
        let col = 62.0;
        for (i, (name, st, lv, color)) in VOICES.iter().enumerate() {
            let cx = x0 + col * (i as f64 + 0.5);
            b.add(p, draw::circle("voice dot", cx - 20.0, 42.0, 3.5, draw::solid(color)));
            ui::silk(b, p, &format!("Voice {name}"), cx + 4.0, 42.0, 7.5, INK, 800.0);
            dial(b, p, &a(st), "Interval", 42.0, (cx, 84.0), color, Some(interval_format()));
            dial(b, p, &a(lv), "Level", 32.0, (cx, 180.0), color, None);
        }
        // ── The dry and the mix ─────────────────────────────────────────
        let dx = x0 + col * 4.0 + 16.0;
        b.add(p, draw::rect("divider", dx - 8.0, 34.0, 1.0, H - 50.0, draw::solid("#ffffff14")));
        dial(b, p, &a("dry"), "Dry", 30.0, (dx + 18.0, 84.0), "#f7f5ef", None);
        dial(b, p, &a("mix"), "Mix", 30.0, (dx + 18.0, 180.0), "#f7f5ef", None);
        // The unit's own On, top right of its column.
        // Its On key in the header: the top-right corner is the host's.
        kit::bypass(b, p, NS, 196.0, 18.0, 18.0, "amber", "On");

        // Responsive: the ladder takes the width; the controls pin right.
        kit::responsive(b, p, &[v]);
    });
    let semis = |id: &str, name: &str, d: f64| ui::param(&a(id), name, Range::linear(-24.0, 24.0, Unit::None), d);
    let level = |id: &str, name: &str, d: f64| ui::param(&a(id), name, Range::linear(0.0, 1.0, Unit::None), d);
    let params = vec![
        semis("semitones", "Voice A", 12.0).featured(),
        level("a_level", "Voice A level", 0.7).featured(),
        semis("b_semitones", "Voice B", -12.0),
        level("b_level", "Voice B level", 0.7),
        semis("c_semitones", "Voice C", 7.0),
        level("c_level", "Voice C level", 0.0),
        semis("d_semitones", "Voice D", 4.0),
        level("d_level", "Voice D level", 0.0),
        level("dry", "Dry", 1.0),
        level("mix", "Mix", 0.5),
        ui::toggle(&a("on"), "Pitch", false),
    ];
    Face { file: "33-pitch-voices", plugin: PLUGIN, scene, params, streams: Vec::new() }
}

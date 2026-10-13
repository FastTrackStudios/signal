//! 480: the classic two-engine digital effects system of the big studios,
//! as its engineers knew it — not the rack mainframe but its remote, the
//! wedge of ivory plastic on the console's meter bridge. A long red LED
//! dot-matrix window across the top under smoked glass, the program
//! loaded in it (its bank and number, its name) and over each of the six
//! sliders the parameter that slider has and its value; the six sliders
//! along the bottom, their caps numbered; and the soft keys, each with
//! its LED, for the program, the bank and the machine.
//!
//! It is a machine, not an algorithm: the host puts it on any reverb,
//! whichever algorithm runs inside, the way the unit loads programs. The
//! program's name is the host's (`verb/program`, text); while it names
//! none the window reads the algorithm running. The program number is
//! the algorithm's place in the block, as bank.program.
//!
//! The six sliders are the reverb's main controls: Decay (the Time knob,
//! its time read out in the window and TIME | BEAT on two soft keys), Size,
//! Pre-Delay, HF Damp, Mod and Level. The back is page 2 — the advanced
//! controls on the same sliders under their own strip of the window.
//!
//! Two finishes: light, the remote's ivory; dark, a charcoal moulding with
//! putty caps. Under the GPU light the moulding has a satin sheen, the
//! window its glass and its LEDs bloom; nothing here reads the clock.

use document_proto::{CornerRadii, NodeId, NodeKind, TextAlign, TextCase};
use surface_kit::draw::{self, family};
use surface_proto::{
    Binding, ButtonMode, Control, ControlKind, Orientation, ParamInfo, Range, SlotRole, Unit,
    ValueFormat,
};
use surface_widgets::Builder;

use crate::face::Face;
use crate::reverb::ui;
use crate::rig_time::kit::{self, VERB, VERB_ALGOS};
use crate::rigkit::lcd;
use crate::rigkit::lettering::{self, Letter, cap_middle, set_width, text_in};

/// The machine's name, as the nameplate letters it.
const NAME: &str = "480";
/// The window's LEDs.
const LED: &str = "#ff4630";
/// The remote's front lip (the wedge's front face), under the moulding.
const LIP: f64 = 7.0;

/// One finish's colours.
struct Look {
    /// The moulding, top to front.
    body: [(f64, &'static str); 3],
    /// Its edge, and the front lip.
    rim: &'static str,
    lip: [(f64, &'static str); 2],
    ink: &'static str,
    dim: &'static str,
    /// The soft keys and their legends.
    key: [(f64, &'static str); 2],
    key_ink: &'static str,
    /// The slider caps, their numerals and index, the slots.
    cap: [(f64, &'static str); 3],
    cap_ink: &'static str,
    slot: &'static str,
    slot_edge: &'static str,
    /// The nameplate's preset.
    accent: &'static str,
    /// The moulding's sheen.
    sheen: f64,
}

const LIGHT: Look = Look {
    body: [(0.0, "#eee8d7"), (0.55, "#e3dac4"), (1.0, "#d5cab0")],
    rim: "#a3967b",
    lip: [(0.0, "#c3b699"), (1.0, "#a99b7f")],
    ink: "#3b3229",
    dim: "#7b6f5d",
    key: [(0.0, "#d6cdb8"), (1.0, "#bcb19a")],
    key_ink: "#2f2821",
    cap: [(0.0, "#55524e"), (0.5, "#2c2a28"), (1.0, "#1b1a19")],
    cap_ink: "#f2ede1",
    slot: "#1d1914",
    slot_edge: "#ffffff80",
    accent: "#a2281a",
    sheen: 0.65,
};

const DARK: Look = Look {
    body: [(0.0, "#3a3733"), (0.55, "#2d2a27"), (1.0, "#22201e")],
    rim: "#0f0e0d",
    lip: [(0.0, "#1d1b19"), (1.0, "#121110")],
    ink: "#e5ddcc",
    dim: "#9e947f",
    key: [(0.0, "#4d4944"), (1.0, "#36332f")],
    key_ink: "#eee6d4",
    cap: [(0.0, "#f2ebdb"), (0.5, "#ddd4bf"), (1.0, "#c0b59c")],
    cap_ink: "#2a241e",
    slot: "#070605",
    slot_edge: "#ffffff1c",
    accent: LED,
    sheen: 0.35,
};

/// The six sliders: (param, legend, the window's label). Decay is the Time
/// knob.
const MAIN: [(&str, &str, &str); 6] = [
    ("time_knob", "Decay", "RT"),
    ("size", "Size", "SIZE"),
    ("predelay", "Pre-Delay", "PDLY"),
    ("damping", "HF Damp", "HF DMP"),
    ("modulation", "Mod", "MOD"),
    ("level", "Level", "LEVEL"),
];

fn a(id: &str) -> String {
    format!("{VERB}/{id}")
}

/// The program's name, as the host names it (text).
#[must_use]
pub fn program_param() -> ParamInfo {
    ui::param(
        &a("program"),
        "Program",
        Range::linear(0.0, 1.0, Unit::None),
        0.0,
    )
    .text()
}

/// The window's label for `id`: the main six's, else its legend.
fn label_of(id: &str, legend: &str) -> String {
    MAIN.iter()
        .find(|(m, _, _)| *m == id)
        .map_or_else(|| legend.to_uppercase(), |(_, _, l)| (*l).to_string())
}

/// What the window reads for `id`: the time the host reads out for the
/// Time knob, the param itself for the rest.
fn shown(id: &str) -> String {
    if id == "time_knob" {
        a("time_text")
    } else {
        a(id)
    }
}

/// How the window prints `id`: the size in metres, the rest as numbers.
fn format_of(id: &str) -> ValueFormat {
    match id {
        "size" => {
            let metres: Vec<String> = (0..=32).map(|i| format!("{}m", 8 + 2 * i)).collect();
            let refs: Vec<&str> = metres.iter().map(String::as_str).collect();
            ValueFormat::labels(&refs)
        }
        "predelay" | "duck_threshold" | "duck_release" => ValueFormat::decimals(0),
        "damping" | "modulation" | "duck" => ValueFormat::decimals(2).unit(""),
        "tone" | "pan_a" => ValueFormat::decimals(2).unit("").signed(),
        "level" => ValueFormat::decimals(1),
        _ => ValueFormat::default(),
    }
}

/// The program numbers: each algorithm's place as bank.program, eight to
/// a bank.
fn numbers() -> Vec<String> {
    (0..VERB_ALGOS.len())
        .map(|i| format!("{}.{}", i / 8 + 1, i % 8 + 1))
        .collect()
}

// ── The moulding ────────────────────────────────────────────────────────

/// The remote, `w` × `h`: the slanted moulding (its sheen a GPU effect)
/// over the front lip. Returns the panel the parts go on.
fn body(b: &mut Builder<'_>, root: NodeId, (w, h): (f64, f64), l: &Look) -> NodeId {
    let mut panel = draw::frame("Remote", 0.0, 0.0, w, h);
    panel.fills = vec![draw::solid(l.rim)];
    panel.corner_radii = CornerRadii::uniform(7.0);
    let panel = b.add(root, panel);
    let lip = draw::rounded(
        draw::rect(
            "lip",
            1.5,
            h - 1.5 - LIP - 4.0,
            w - 3.0,
            LIP + 4.0,
            draw::linear(180.0, &l.lip),
        ),
        5.5,
    );
    b.add(panel, lip);
    let mut top = draw::frame("Moulding", 1.5, 1.5, w - 3.0, h - 3.0 - LIP);
    top.fills = vec![draw::linear(180.0, &l.body)];
    top.corner_radii = CornerRadii::uniform(6.0);
    top.effects = vec![
        draw::inner_shadow("#ffffff55", 0.0, 1.0, 0.0),
        draw::inner_shadow("#00000024", 0.0, -1.5, 1.0),
    ];
    let top = b.add(panel, top);
    // A moulding's faint texture: the tool's fine horizontal finish.
    let grain: Vec<Vec<(f64, f64)>> = (0..((h - LIP) / 2.5) as i32)
        .map(|i| {
            vec![
                (0.0, 1.0 + f64::from(i) * 2.5),
                (w - 3.0, 1.0 + f64::from(i) * 2.5),
            ]
        })
        .collect();
    b.add(
        top,
        ui::strokes("finish", &grain, 0.5, draw::solid(&ui::alpha(l.ink, 0.025))),
    );
    let sheen = b
        .scene
        .add_asset("text/wgsl", include_bytes!("plastic.wgsl").to_vec());
    kit::effect(
        b,
        top,
        &format!("asset:{}", sheen.0),
        &[("strength", l.sheen), ("amount", 0.5)],
    );
    panel
}

/// The machine's lettering: "480" wide and heavy, its caps' middle at
/// (`x`, `cy`) from the left. Returns its width.
fn mark(b: &mut Builder<'_>, p: NodeId, (x, cy): (f64, f64), size: f64, ink: &str) -> f64 {
    let l = Letter::new(size, 800.0, ink)
        .family(family::SAIRA)
        .stretch(1.25)
        .track(0.02);
    lettering::letter_aligned(b, p, NAME, (x, cy), l, TextAlign::Left);
    set_width(NAME, &l)
}

/// The line under (or beside) the mark.
fn system(b: &mut Builder<'_>, p: NodeId, (x, cy): (f64, f64), size: f64, ink: &str) {
    let l = Letter::new(size, 600.0, ink)
        .family(family::SAIRA)
        .stretch(1.1)
        .track(0.22);
    lettering::letter_aligned(b, p, "DIGITAL EFFECTS SYSTEM", (x, cy), l, TextAlign::Left);
}

/// The nameplate's title: the mark, small.
fn title(b: &mut Builder<'_>, t: NodeId, h: f64, size: f64, ink: &str) {
    mark(b, t, (4.0, h / 2.0), size, ink);
}

// ── The window ──────────────────────────────────────────────────────────

/// The LED window at (`x`, `y`, `w`, `h`) on `p`: smoked red glass in its
/// black bezel. Returns the window to draw in; [`glaze`] it when done.
fn window(b: &mut Builder<'_>, p: NodeId, (x, y, w, h): (f64, f64, f64, f64)) -> NodeId {
    ui::display(b, p, "Display", x, y, w, h, LED, "")
}

/// The glass over a [`window`]: the pane on the GPU, a sheen without.
fn glaze(b: &mut Builder<'_>, win: NodeId) {
    kit::glass(b, win);
    ui::glass(b, win);
}

/// The window's dot-matrix face at `size`.
fn dots(size: f64, color: &str) -> Letter<'_> {
    lcd::dots(size, color).weight(850.0)
}

/// One cell's advance at `size`.
fn pitch(size: f64) -> f64 {
    let l = dots(size, LED);
    0.6 * size + l.track * size
}

/// The unlit dots of `cells` character cells (5 × 7 each), the first at
/// `x`, their caps' middle on `cy`: where a lit glyph's dots fall.
fn ghost(b: &mut Builder<'_>, win: NodeId, (x, cy): (f64, f64), cells: usize, size: f64) {
    let (w, h) = b
        .scene
        .node(win)
        .map_or((0.0, 0.0), |n| (n.size.width, n.size.height));
    let r = 0.036 * size;
    let step = pitch(size);
    let mut d = String::new();
    for i in 0..cells {
        // The renderer sets a glyph 1 px in.
        let x0 = x + 1.0 + step * i as f64;
        for col in 0..5 {
            for row in 0..7 {
                let (dx, dy) = (
                    x0 + size * (0.05 + 0.1 * f64::from(col)),
                    cy + size * (0.3 - 0.1 * f64::from(row)),
                );
                d.push_str(&format!(
                    "M {:.2} {:.2} h {:.2} v {:.2} h {:.2} Z ",
                    dx - r,
                    dy - r,
                    2.0 * r,
                    2.0 * r,
                    -2.0 * r
                ));
            }
        }
    }
    b.add(
        win,
        ui::area("unlit dots", d, w, h, draw::solid(&ui::alpha(LED, 0.075))),
    );
}

/// A lit text node in the window's face, set from the left of its box.
fn lit_node(s: &str, w: f64, size: f64, color: &str, h: f64, upper: bool) -> document_proto::Node {
    let l = dots(size, color);
    let mut t = text_in(s, (0.0, h / 2.0 - cap_middle(&l), w), &l, TextAlign::Left);
    t.name = "value".into();
    if upper && let NodeKind::Text(d) = &mut t.kind {
        d.text_case = TextCase::Upper;
    }
    draw::emissive(t, draw::hex(color), 1.4, size * 0.8)
}

/// A readout of `binding` in the window, `cells` cells from `x`, its caps'
/// middle on `cy`, over the cells' unlit dots.
fn led(
    b: &mut Builder<'_>,
    win: NodeId,
    name: &str,
    binding: Binding,
    (x, cy, size, cells): (f64, f64, f64, usize),
    format: ValueFormat,
    upper: bool,
) -> NodeId {
    ghost(b, win, (x, cy), cells, size);
    led_bare(b, win, name, binding, (x, cy, size, cells), format, upper)
}

/// [`led`] without its unlit dots (drawn already).
fn led_bare(
    b: &mut Builder<'_>,
    win: NodeId,
    name: &str,
    binding: Binding,
    (x, cy, size, cells): (f64, f64, f64, usize),
    format: ValueFormat,
    upper: bool,
) -> NodeId {
    let w = pitch(size) * cells as f64 + 2.0;
    let h = (size * 1.6).ceil();
    let root = b.add(win, draw::frame(name, x, cy - h / 2.0, w, h));
    let value = b.add(root, lit_node("", w, size, LED, h, upper));
    let c = Control::new(ControlKind::Readout)
        .slot(SlotRole::Value, value.0)
        .bound(binding)
        .formatted(format);
    b.controls.insert(root, c);
    root
}

/// A fixed legend in the window, dimmer than a value.
fn legend(b: &mut Builder<'_>, win: NodeId, s: &str, (x, cy): (f64, f64), size: f64) {
    let w = pitch(size) * s.chars().count() as f64 + 4.0;
    let h = (size * 1.6).ceil();
    let mut n = lit_node(s, w, size, &ui::alpha(LED, 0.62), h, false);
    n.name = format!("legend {s}");
    n.transform.tx = x;
    n.transform.ty = cy - h / 2.0;
    b.add(win, n);
}

/// The program, from `x`: on the first line "PROG" and its number, on the
/// second (`gap` under) its name in `name_size` over `cells` cells — the
/// host's, or while it names none the algorithm running.
fn program(
    b: &mut Builder<'_>,
    win: NodeId,
    (x, cy): (f64, f64),
    gap: f64,
    (name_size, cells): (f64, usize),
) {
    let small = (name_size * 0.62).max(7.0);
    legend(b, win, "PROG", (x, cy), small);
    let nums = numbers();
    let refs: Vec<&str> = nums.iter().map(String::as_str).collect();
    led(
        b,
        win,
        "program number",
        ui::bind(&a("algorithm")),
        (x + pitch(small) * 5.0, cy, small, 3),
        ValueFormat::labels(&refs),
        false,
    );
    // The name.
    let ny = cy + gap;
    ghost(b, win, (x, ny), cells, name_size);
    let w = pitch(name_size) * cells as f64 + 2.0;
    let h = (name_size * 1.6).ceil();
    let root = b.add(win, draw::frame("program", x, ny - h / 2.0, w, h));
    let value = b.add(root, lit_node("", w, name_size, LED, h, true));
    // While the host names no program: the algorithm running.
    let none = b.add(root, draw::frame("no program", 0.0, 0.0, w, h));
    led_bare(
        b,
        none,
        "algorithm",
        ui::bind(&a("algorithm")),
        (0.0, h / 2.0, name_size, cells),
        ValueFormat::default(),
        true,
    );
    let c = Control::new(ControlKind::Readout)
        .slot(SlotRole::Value, value.0)
        .slot(SlotRole::Placeholder, none.0)
        .bound(ui::bind(&a("program")));
    b.controls.insert(root, c);
}

/// The window's columns, one over each slider: the slider's label and,
/// under it (`gap` down), its value — `cols` as (centre in the window,
/// param, legend), each `cells` wide (the first `first` wide: a time reads
/// longer).
fn columns(
    b: &mut Builder<'_>,
    win: NodeId,
    cols: &[(f64, &str, &str)],
    cy: f64,
    gap: f64,
    (label_size, value_size): (f64, f64),
    (cells, first): (usize, usize),
) {
    for (i, (cx, id, leg)) in cols.iter().enumerate() {
        let n = if i == 0 { first } else { cells };
        let x = cx - pitch(value_size) * cells as f64 / 2.0;
        legend(b, win, &label_of(id, leg), (x, cy), label_size);
        led(
            b,
            win,
            &format!("{id} value"),
            ui::bind(&shown(id)),
            (x, cy + gap, value_size, n),
            format_of(id),
            false,
        );
    }
}

// ── The controls ────────────────────────────────────────────────────────

/// A slider's proportions: its cap, its travel, and where its legend (and
/// its value, printed, if any) go below its top.
#[derive(Clone, Copy)]
struct Slide {
    cap: (f64, f64),
    travel: f64,
    legend: f64,
    value: Option<f64>,
    size: f64,
}

/// A slider on `id`, its cap numbered `num`, centred on `cx` from `top`:
/// a dark slot cut through the moulding, its scale beside it, the cap
/// riding in it; its legend printed under it.
fn slider(
    b: &mut Builder<'_>,
    p: NodeId,
    (id, leg): (&str, &str),
    num: usize,
    (cx, top): (f64, f64),
    s: Slide,
    l: &Look,
) -> NodeId {
    let (cw, ch) = s.cap;
    // A finger's width at least, whatever the cap.
    let w = (cw + 18.0).max(44.0);
    let lane_h = s.travel + ch;
    let root = b.add(p, draw::frame(leg, cx - w / 2.0, top, w, lane_h));
    let slot = draw::rounded(
        draw::rect(
            "slot",
            w / 2.0 - 2.5,
            ch / 2.0 - 3.0,
            5.0,
            s.travel + 6.0,
            draw::solid(l.slot),
        ),
        2.5,
    );
    b.add(
        root,
        draw::with_effects(
            slot,
            vec![
                draw::inner_shadow("#000000c0", 0.0, 1.0, 2.0),
                draw::drop_shadow(l.slot_edge, 0.0, 1.0, 0.0),
            ],
        ),
    );
    let ticks: Vec<Vec<(f64, f64)>> = (0..=10)
        .map(|i| {
            let y = ch / 2.0 + s.travel * f64::from(i) / 10.0;
            let len = if i % 5 == 0 { 5.0 } else { 3.0 };
            vec![(w / 2.0 - 6.0 - len, y), (w / 2.0 - 6.0, y)]
        })
        .collect();
    b.add(
        root,
        ui::strokes("scale", &ticks, 0.8, draw::solid(&ui::alpha(l.ink, 0.5))),
    );
    let lane = b.add(
        root,
        draw::frame("lane", w / 2.0 - cw / 2.0, 0.0, cw, lane_h),
    );
    let cap = b.add(lane, draw::frame("cap", 0.0, 0.0, cw, ch));
    let shell = draw::rounded(
        draw::rect("shell", 0.0, 0.0, cw, ch, draw::linear(180.0, &l.cap)),
        2.0,
    );
    b.add(
        cap,
        draw::with_effects(
            shell,
            vec![
                draw::drop_shadow("#00000080", 0.0, 2.0, 3.0),
                draw::inner_shadow("#ffffff38", 0.0, 1.0, 0.0),
            ],
        ),
    );
    // The index either side of the numeral.
    for x in [1.5, cw - 5.5] {
        b.add(
            cap,
            draw::rect("index", x, ch / 2.0 - 0.6, 4.0, 1.2, draw::solid(l.cap_ink)),
        );
    }
    ui::silk(
        b,
        cap,
        &num.to_string(),
        cw / 2.0,
        ch / 2.0,
        (ch * 0.62).min(10.0),
        l.cap_ink,
        800.0,
    );
    let c = Control::new(ControlKind::Fader {
        orientation: Orientation::Vertical,
    })
    .slot(SlotRole::Thumb, cap.0)
    .bound(ui::bind(&a(id)));
    b.controls.insert(root, c);
    ui::silk(b, p, leg, cx, top + s.legend, s.size, l.ink, 800.0);
    if let Some(dy) = s.value {
        printed_value(b, p, id, (cx, top + dy), s.size * 0.95, l.dim);
    }
    root
}

/// A slider's value, printed small under its legend.
fn printed_value(
    b: &mut Builder<'_>,
    p: NodeId,
    id: &str,
    (cx, cy): (f64, f64),
    size: f64,
    ink: &str,
) {
    let (w, h) = (64.0, (size * 1.4).ceil());
    let root = b.add(
        p,
        draw::frame(&format!("{id} readout"), cx - w / 2.0, cy - h / 2.0, w, h),
    );
    let t = b.add(
        root,
        ui::text(
            "value",
            "",
            0.0,
            h / 2.0,
            w,
            size,
            ink,
            TextAlign::Center,
            650.0,
            false,
        ),
    );
    let c = Control::new(ControlKind::Readout)
        .slot(SlotRole::Value, t.0)
        .bound(ui::bind(&shown(id)))
        .formatted(format_of(id));
    b.controls.insert(root, c);
}

/// A row of sliders: `items` centred on `xs` from `top`, numbered from
/// `first`.
fn sliders(
    b: &mut Builder<'_>,
    p: NodeId,
    items: &[(&str, &str)],
    (xs, top): (&[f64], f64),
    first: usize,
    s: Slide,
    l: &Look,
) {
    for (i, (item, x)) in items.iter().zip(xs).enumerate() {
        slider(b, p, *item, first + i, (*x, top), s, l);
    }
}

/// A soft key `kw` × `kh` with its LED over it (the key's top at `y`): the
/// control `c` with the LED as its lamp.
fn key(
    b: &mut Builder<'_>,
    p: NodeId,
    leg: &str,
    (x, y, kw, kh): (f64, f64, f64, f64),
    size: f64,
    l: &Look,
    c: Control,
) -> NodeId {
    let root = b.add(
        p,
        draw::frame(&format!("key {leg}"), x, y - 9.0, kw, kh + 9.0),
    );
    b.add(
        root,
        draw::rounded(
            draw::rect(
                "led window",
                kw / 2.0 - 5.0,
                0.0,
                10.0,
                5.0,
                draw::solid("#1c0806"),
            ),
            1.2,
        ),
    );
    let lamp = b.add(
        root,
        draw::emissive(
            draw::rounded(
                draw::rect("led", kw / 2.0 - 4.0, 0.8, 8.0, 3.4, draw::solid(LED)),
                1.0,
            ),
            draw::hex(LED),
            1.3,
            kit::GLOW,
        ),
    );
    b.add(
        root,
        draw::rounded(
            draw::rect(
                "key well",
                -1.2,
                7.8,
                kw + 2.4,
                kh + 2.4,
                draw::solid("#00000038"),
            ),
            3.5,
        ),
    );
    let cap = draw::rounded(
        draw::rect("key", 0.0, 9.0, kw, kh, draw::linear(180.0, &l.key)),
        3.0,
    );
    b.add(
        root,
        draw::with_effects(
            cap,
            vec![
                draw::drop_shadow("#00000060", 0.0, 1.5, 2.0),
                draw::inner_shadow("#ffffff50", 0.0, 1.0, 0.0),
            ],
        ),
    );
    ui::silk(
        b,
        root,
        leg,
        kw / 2.0,
        9.0 + kh / 2.0,
        size,
        l.key_ink,
        800.0,
    );
    b.controls.insert(root, c.slot(SlotRole::Led, lamp.0));
    root
}

/// A key that presses `id` (a nameplate press).
fn press(id: &str) -> Control {
    Control::new(ControlKind::Button {
        mode: ButtonMode::Momentary,
    })
    .bound(ui::bind(&a(id)))
}

/// TIME | BEAT: two keys over `sync`.
fn sync(i: u32) -> Control {
    Control::new(ControlKind::Button {
        mode: ButtonMode::Select { index: i, count: 2 },
    })
    .bound(ui::bind(&a("sync")))
}

/// The six soft keys in two rows of three from (`x`, `y`), `kw` × `kh`
/// with `gap` between and `row` from row to row: the program's (bank,
/// previous, next) over TIME, BEAT and the machine.
fn soft_keys(
    b: &mut Builder<'_>,
    p: NodeId,
    (x, y): (f64, f64),
    (kw, kh, gap, row): (f64, f64, f64, f64),
    size: f64,
    l: &Look,
) {
    let keys: [(&str, Control); 6] = [
        ("Bank", press("preset_browse")),
        ("Prev", press("preset_prev")),
        ("Next", press("preset_next")),
        ("Time", sync(0)),
        ("Beat", sync(1)),
        ("Mach", press("algo_menu")),
    ];
    for (i, (leg, c)) in keys.into_iter().enumerate() {
        let (cx, ry) = ((i % 3) as f64, (i / 3) as f64);
        key(
            b,
            p,
            leg,
            (x + cx * (kw + gap), y + ry * row, kw, kh),
            size,
            l,
            c,
        );
    }
}

/// The bypass, as a soft key, where every Time face has its bypass: lit
/// while the reverb plays.
fn bypass(b: &mut Builder<'_>, p: NodeId, (cx, cy, d): (f64, f64, f64), size: f64, l: &Look) {
    let c = Control::new(ControlKind::Button {
        mode: ButtonMode::Toggle,
    })
    .bound(ui::bind(&a("on")));
    key(
        b,
        p,
        "In",
        (cx - d / 2.0, cy - d / 2.0 + 4.0, d, d - 4.0),
        size,
        l,
        c,
    );
}

/// `n` centres `step` apart from `x0`.
fn spread(x0: f64, step: f64, n: usize) -> Vec<f64> {
    (0..n).map(|i| x0 + step * i as f64).collect()
}

fn main_items() -> Vec<(&'static str, &'static str)> {
    MAIN.iter().map(|(id, leg, _)| (*id, *leg)).collect()
}

// ── The faces ───────────────────────────────────────────────────────────

#[must_use]
pub fn face(dark: bool) -> Face {
    face_at(dark, kit::Tier::Desktop)
}

/// The face at `tier`: the window across the top over the six sliders,
/// the mark beside them, the soft keys under the nameplate. On a phone the
/// window keeps to the program and the time at the left, TIME | BEAT under
/// it, and the six sliders take finger-sized caps.
#[must_use]
pub fn face_at(dark: bool, tier: kit::Tier) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane(NAME, tier, |b, root| {
        let p = body(b, root, (w, kit::H), l);
        if small_room {
            kit::nameplate_top_right(
                b,
                p,
                VERB,
                tier,
                (6.0, 30.0),
                70.0,
                220.0,
                12.0,
                (l.accent, l.dim),
                |b, t| title(b, t, 30.0, 17.0, l.ink),
            );
            let mw = mark(b, p, (214.0, 22.0), 22.0, l.ink);
            system(b, p, (222.0 + mw, 23.0), 6.0, l.dim);
            let (dx, dy, dw, dh) = (16.0, 12.0, 170.0, 80.0);
            let win = window(b, p, (dx, dy, dw, dh));
            program(b, win, (7.0, 13.0), 21.0, (12.0, 19));
            legend(b, win, "RT", (7.0, 62.0), 8.0);
            led(
                b,
                win,
                "time_knob value",
                ui::bind(&a("time_text")),
                (7.0 + pitch(8.0) * 3.0, 62.0, 10.0, 21),
                ValueFormat::default(),
                false,
            );
            glaze(b, win);
            for (i, leg) in ["Time", "Beat"].iter().enumerate() {
                key(
                    b,
                    p,
                    leg,
                    (dx + 90.0 * i as f64, 112.0, 80.0, 30.0),
                    8.0,
                    l,
                    sync(i as u32),
                );
            }
            let s = Slide {
                cap: (34.0, 20.0),
                travel: 64.0,
                legend: 98.0,
                value: None,
                size: 7.0,
            };
            sliders(
                b,
                p,
                &main_items(),
                (&spread(236.0, 72.0, 6), 44.0),
                1,
                s,
                l,
            );
        } else {
            kit::nameplate_top_right(
                b,
                p,
                VERB,
                tier,
                (12.0, 26.0),
                64.0,
                200.0,
                10.0,
                (l.accent, l.dim),
                |b, t| title(b, t, 26.0, 14.0, l.ink),
            );
            let (dx, dy, dw, dh) = (18.0, 13.0, 624.0, 48.0);
            let xs = spread(262.0, 68.0, 6);
            let win = window(b, p, (dx, dy, dw, dh));
            program(b, win, (8.0, 14.0), 19.0, (12.0, 16));
            ui::rule(b, win, 186.0, 6.0, 0.8, dh - 12.0, &ui::alpha(LED, 0.22));
            let cols: Vec<(f64, &str, &str)> = MAIN
                .iter()
                .zip(&xs)
                .map(|((id, leg, _), x)| (x - dx, *id, *leg))
                .collect();
            columns(b, win, &cols, 14.0, 19.0, (7.5, 9.0), (10, 10));
            glaze(b, win);
            let mw = mark(b, p, (22.0, 98.0), 40.0, l.ink);
            system(b, p, (24.0, 126.0), 6.0, l.dim);
            ui::rule(
                b,
                p,
                24.0,
                136.0,
                mw.max(150.0),
                1.0,
                &ui::alpha(l.ink, 0.35),
            );
            let s = Slide {
                cap: (24.0, 14.0),
                travel: 52.0,
                legend: 76.0,
                value: None,
                size: 6.5,
            };
            sliders(b, p, &main_items(), (&xs, 72.0), 1, s, l);
            soft_keys(b, p, (664.0, 58.0), (62.0, 20.0, 8.0, 50.0), 6.5, l);
        }
        let (bx, by, bd) = kit::bypass_spot(tier);
        bypass(b, p, (bx, by, bd), if small_room { 8.0 } else { 6.5 }, l);
        kit::flip_arrow(b, p, VERB, tier, false, (l.ink, &ui::alpha(l.dim, 0.25)));
    });
    let mut f = kit::reverb_face(kit::tier_file("96-verb-480", tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::time_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f.params.push(program_param());
    f
}

/// Its back at `tier`: page 2 — the advanced controls on the same
/// sliders, numbered on from the front's six, under their own strip of the
/// window (on a phone, their values printed under them); the nameplate,
/// the bypass and the arrow where they are on the front.
#[must_use]
pub fn back_at(dark: bool, tier: kit::Tier) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let small_room = matches!(tier, kit::Tier::Phone | kit::Tier::Strip);
    let w = tier.lane_w();
    let scene = kit::tier_lane(NAME, tier, |b, root| {
        let p = body(b, root, (w, kit::H), l);
        let items: Vec<(&str, &str)> = kit::VERB_MORE.to_vec();
        if small_room {
            let mw = mark(b, p, (16.0, 22.0), 22.0, l.ink);
            system(b, p, (24.0 + mw, 23.0), 6.0, l.dim);
            let s = Slide {
                cap: (34.0, 20.0),
                travel: 60.0,
                legend: 92.0,
                value: Some(103.0),
                size: 7.0,
            };
            sliders(b, p, &items[..7], (&spread(52.0, 88.0, 7), 42.0), 7, s, l);
        } else {
            let (dx, dy, dw, dh) = (12.0, 13.0, 650.0, 48.0);
            let xs = spread(52.0, 64.0, 10);
            let win = window(b, p, (dx, dy, dw, dh));
            let cols: Vec<(f64, &str, &str)> = items
                .iter()
                .zip(&xs)
                .map(|((id, leg), x)| (x - dx, *id, *leg))
                .collect();
            columns(b, win, &cols, 14.0, 19.0, (7.5, 9.0), (10, 10));
            glaze(b, win);
            let s = Slide {
                cap: (24.0, 14.0),
                travel: 52.0,
                legend: 76.0,
                value: None,
                size: 6.5,
            };
            sliders(b, p, &items, (&xs, 72.0), 7, s, l);
            mark(b, p, (684.0, 112.0), 24.0, l.ink);
            system(b, p, (686.0, 130.0), 5.0, l.dim);
            ui::silk_left(b, p, "Page 2 · Advanced", 684.0, 144.0, 6.5, l.ink, 800.0);
        }
        let (np, title_size, size) = if small_room {
            ((6.0, 30.0), 17.0, 12.0)
        } else {
            ((12.0, 26.0), 14.0, 10.0)
        };
        let key_size = if small_room { 8.0 } else { 6.5 };
        kit::head(b, p, VERB, true, |b, head| {
            kit::nameplate_top_right(
                b,
                head,
                VERB,
                tier,
                np,
                if small_room { 70.0 } else { 64.0 },
                if small_room { 220.0 } else { 200.0 },
                size,
                (l.accent, l.dim),
                |b, t| title(b, t, np.1, title_size, l.ink),
            );
            bypass(b, head, kit::bypass_spot(tier), key_size, l);
        });
        kit::flip_arrow(b, p, VERB, tier, true, (l.ink, &ui::alpha(l.dim, 0.25)));
    });
    let mut f = kit::reverb_face(kit::tier_file("96-verb-480-back", tier, dark), scene);
    f.params.extend(kit::nameplate_params(VERB));
    f.params.extend(kit::flip_params(VERB));
    f
}

/// Its tall face: the whole remote — the mark and the nameplate along the
/// top, the window, the six sliders with the soft keys beside them and
/// the bypass at the right, and under them page 2's sliders, their values
/// printed.
#[must_use]
pub fn tall(dark: bool) -> Face {
    let l = if dark { &DARK } else { &LIGHT };
    let scene = kit::tall_lane(NAME, |b, root| {
        let p = body(b, root, (kit::TALL_W, kit::TALL_H), l);
        let mw = mark(b, p, (18.0, 30.0), 30.0, l.ink);
        system(b, p, (30.0 + mw, 31.0), 6.5, l.dim);
        kit::nameplate_right(
            b,
            p,
            VERB,
            kit::TALL_W - 10.0,
            (16.0, 30.0),
            70.0,
            200.0,
            12.0,
            (l.accent, l.dim),
            |b, t| title(b, t, 30.0, 17.0, l.ink),
        );
        let (dx, dy, dw, dh) = (16.0, 58.0, 728.0, 56.0);
        let xs = spread(270.0, 76.0, 6);
        let win = window(b, p, (dx, dy, dw, dh));
        program(b, win, (9.0, 16.0), 24.0, (14.0, 15));
        ui::rule(b, win, 196.0, 7.0, 0.8, dh - 14.0, &ui::alpha(LED, 0.22));
        let cols: Vec<(f64, &str, &str)> = MAIN
            .iter()
            .zip(&xs)
            .map(|((id, leg, _), x)| (x - dx, *id, *leg))
            .collect();
        columns(b, win, &cols, 16.0, 22.0, (8.0, 10.0), (10, 10));
        glaze(b, win);
        let s = Slide {
            cap: (28.0, 15.0),
            travel: 58.0,
            legend: 84.0,
            value: None,
            size: 7.0,
        };
        sliders(b, p, &main_items(), (&xs, 130.0), 1, s, l);
        soft_keys(b, p, (18.0, 144.0), (58.0, 22.0, 8.0, 42.0), 7.0, l);
        bypass(b, p, (712.0, 168.0, 44.0), 8.0, l);
        // Page 2: the advanced controls, less the front's.
        let more = kit::tall_more(VERB, &main_items());
        let s2 = Slide {
            cap: (24.0, 13.0),
            travel: 46.0,
            legend: 70.0,
            value: Some(80.0),
            size: 6.5,
        };
        sliders(
            b,
            p,
            &more,
            (&spread(270.0, 70.0, more.len()), 228.0),
            7,
            s2,
            l,
        );
        ui::rule(
            b,
            p,
            16.0,
            220.0,
            kit::TALL_W - 32.0,
            1.0,
            &ui::alpha(l.ink, 0.18),
        );
        ui::silk_left(b, p, "Page 2", 20.0, 244.0, 8.0, l.ink, 800.0);
        ui::silk_left(
            b,
            p,
            "Filters · Ducking · Pan",
            20.0,
            258.0,
            6.0,
            l.dim,
            700.0,
        );
    });
    let mut f = kit::reverb_face(kit::tall_file("96-verb-480", dark), scene);
    f.params.extend(kit::tall_params(VERB));
    f.params.push(program_param());
    f
}

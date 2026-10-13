//! Hardware the redrawn saturator faces (tape, transformer) share: an
//! engraved nameplate badge, a glazed window into the unit, and the
//! knob family — `knob.3d` caps (lit per pixel on the GPU stage, drawn by
//! `knob.render` without it) with the face printing its own scale, legend
//! and value.

use document_proto::NodeId;
use surface_kit::{Place, TOUCH_MIN, draw};
use surface_widgets::Builder;

use crate::delay::art;
use crate::delay::parts::{self, Ink};

/// A row of the family's knobs: the cap (`knob.3d` preset and options),
/// the inks (legend, scale and value), the shafts' height and the
/// legends' and values' baselines.
pub(crate) struct Row<'a> {
    pub look: (&'a str, &'a [&'a str]),
    pub ink: (&'a str, &'a str),
    pub y: f64,
    pub legend_y: f64,
    pub value_y: f64,
}

/// One knob of a row: its address, legend, shaft x, diameter and the
/// scale printed round it.
pub(crate) type Spec<'a> = (&'a str, &'a str, f64, f64, &'a [(f64, &'a str)]);

/// A knob of the family on `row`: kit `knob.3d`, bound to `addr`, the
/// face printing its scale (21 ticks), legend and live value.
pub(crate) fn knob(
    b: &mut Builder<'_>,
    panel: NodeId,
    row: &Row<'_>,
    (addr, name, cx, d, marks): Spec<'_>,
) -> NodeId {
    let (variant, opts) = row.look;
    let opts: Vec<&str> = opts.iter().copied().chain(["ticks=none"]).collect();
    let p = Place::new(name)
        .size(d)
        .variant(variant)
        .options(&opts)
        .bind(parts::bind(addr))
        .legend(name)
        .touch(TOUCH_MIN);
    let k = parts::put(b, panel, "knob.3d", &p, cx, row.y);
    parts::strip_legend(b, k);
    parts::strip_scale(b, k);
    let sweep = parts::sweep_of(b, k);
    let (main, dim) = row.ink;
    parts::marks(
        b,
        panel,
        (cx, row.y),
        d / 2.0 + 9.0,
        sweep,
        marks,
        21,
        Ink::new(6.8, dim).weight(700.0).track(0.0).plain(),
    );
    parts::legend_at(
        b,
        k,
        name,
        row.legend_y,
        Ink::new(8.2, main).weight(800.0).track(0.16),
    );
    parts::value_at(
        b,
        k,
        row.value_y,
        Ink::new(7.5, dim).weight(600.0).track(0.02).plain(),
    );
    k
}

/// A detented selector of the family: a black bakelite chicken-head
/// (`knob.3d`) for `addr`, `d` across on (`cx`, `cy`), its positions `labels` printed round it in
/// `ink`. The face prints the legend.
pub(crate) fn selector(
    b: &mut Builder<'_>,
    panel: NodeId,
    (name, addr): (&str, &str),
    (cx, cy, d): (f64, f64, f64),
    labels: &[&str],
    ink: &str,
) -> NodeId {
    let opts: Vec<&str> = labels
        .iter()
        .copied()
        .chain(["color=#121214", "pointer=#ece6da", "spec=1.1", "shadow=1.3"])
        .collect();
    let p = Place::new(name)
        .size(d)
        .variant("bakelite_chicken")
        .options(&opts)
        .bind(parts::bind(addr))
        .legend(name)
        .ink(surface_kit::Ink::Color(ink.into()))
        .keep_case()
        .touch(TOUCH_MIN);
    let k = parts::put(b, panel, "knob.3d", &p, cx, cy);
    parts::strip_legend(b, k);
    k
}

/// An engraved nameplate: a brushed aluminium badge `(x, y, w, h)` held by
/// two screws, the name in black infill and a line under it.
pub(crate) fn badge(
    b: &mut Builder<'_>,
    parent: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    (name, size): (&str, f64),
    line: &str,
) -> NodeId {
    let plate = draw::rounded(
        draw::rect(
            "badge",
            x,
            y,
            w,
            h,
            draw::linear(
                180.0,
                &[
                    (0.0, "#e9eaec"),
                    (0.45, "#c3c6ca"),
                    (0.55, "#b4b8bd"),
                    (1.0, "#9a9ea4"),
                ],
            ),
        ),
        3.0,
    );
    let plate = draw::with_effects(
        draw::stroked(plate, "#00000099", 0.8),
        vec![
            draw::inner_shadow("#ffffffa0", 0.0, 1.0, 0.0),
            draw::inner_shadow("#00000040", 0.0, -1.0, 1.0),
            draw::drop_shadow("#000000a0", 0.0, 1.5, 3.0),
        ],
    );
    let badge = b.add(parent, plate);
    // The brushing: fine horizontal streaks.
    let streaks: Vec<Vec<(f64, f64)>> = (0..)
        .map(|i| 1.6 + 1.9 * f64::from(i))
        .take_while(|yy| *yy < h - 1.0)
        .map(|yy| vec![(2.0, yy), (w - 2.0, yy)])
        .collect();
    b.add(
        badge,
        draw::with_opacity(
            parts::stroke_path(
                "brushing",
                (0.0, 0.0, w, h),
                &streaks,
                0.5,
                "#ffffff",
                false,
            ),
            0.18,
        ),
    );
    // Engraved: a light lip under each black-filled letter.
    let cy = h * 0.42;
    parts::silk(
        b,
        badge,
        name,
        w / 2.0,
        cy + 0.8,
        Ink::new(size, "#ffffff90").weight(900.0).track(0.12),
    );
    parts::silk(
        b,
        badge,
        name,
        w / 2.0,
        cy,
        Ink::new(size, "#141517").weight(900.0).track(0.12),
    );
    parts::rule(b, badge, 14.0, w - 14.0, h * 0.64, 0.8, "#14151780");
    parts::silk(
        b,
        badge,
        line,
        w / 2.0,
        h * 0.8,
        Ink::new(5.6, "#2b2d30").weight(800.0).track(0.26),
    );
    for sx in [7.0, w - 7.0] {
        art::screw(b, badge, (sx, h / 2.0), 2.6, 40.0);
    }
    badge
}

/// A glazed window into the unit at `(x, y, w, h)`: a machined bezel and,
/// sunk in it, the dark interior `name` (returned) — the subtree the GPU
/// stage lays its glass over.
pub(crate) fn window(
    b: &mut Builder<'_>,
    parent: NodeId,
    name: &str,
    (x, y, w, h): (f64, f64, f64, f64),
    interior: &[(f64, &str)],
) -> NodeId {
    let bezel = draw::rounded(
        draw::rect(
            "bezel",
            x - 5.0,
            y - 5.0,
            w + 10.0,
            h + 10.0,
            draw::linear(
                180.0,
                &[
                    (0.0, "#6c7075"),
                    (0.12, "#3a3d41"),
                    (0.88, "#1d1f22"),
                    (1.0, "#0e0f10"),
                ],
            ),
        ),
        9.0,
    );
    b.add(
        parent,
        draw::with_effects(
            draw::stroked(bezel, "#000000b0", 1.0),
            vec![
                draw::inner_shadow("#ffffff30", 0.0, 1.0, 0.0),
                draw::drop_shadow("#00000080", 0.0, 2.0, 4.0),
            ],
        ),
    );
    let mut win = draw::frame(name, x, y, w, h);
    win.fills = vec![draw::linear(180.0, interior)];
    let win = draw::with_effects(
        draw::stroked(draw::rounded(win, 5.0), "#000000", 1.0),
        vec![draw::inner_shadow("#000000e0", 0.0, 3.0, 9.0)],
    );
    let mut win = win;
    win.clips_content = true;
    b.add(parent, win)
}

/// The window's depth: a shade inside its top edge, cast by the bezel,
/// drawn last over what is in it.
pub(crate) fn window_lip(b: &mut Builder<'_>, win: NodeId) {
    let (w, h) = parts::size_of(b, win);
    b.add(
        win,
        draw::rect(
            "bezel shade",
            0.0,
            0.0,
            w,
            14.0,
            draw::linear(180.0, &[(0.0, "#000000a0"), (1.0, "#00000000")]),
        ),
    );
    b.add(
        win,
        draw::rect(
            "bezel shade left",
            0.0,
            0.0,
            8.0,
            h,
            draw::linear(90.0, &[(0.0, "#00000080"), (1.0, "#00000000")]),
        ),
    );
}

//! Piano-key push buttons: wide, low, square-cornered white caps in a
//! black housing, unlit, read by which one is *in* — the switching of a
//! 1970s desktop compressor (its POWER key and its interlocked METER row).
//! Legends are printed on the panel above the caps, not on them.
//!
//! `button.piano` is one latching key over a toggle param;
//! `button.piano_bank` butts one key per `Place::options` entry into a
//! single housing, each a `Select` button over a choice param. The pressed
//! overlay (the cap's shadow moving into it) is the `Led` slot.

use document_proto::{NodeId, TextAlign};
use surface_proto::{ButtonMode, Control, ControlKind, SlotRole};
use surface_widgets::Builder;

use super::parts::{PANEL_INK, cased, ink, lin, shade, tint};
use crate::draw;
use crate::registry::{BuildFn, ComponentDef, Family, Place};

/// Cap height as a fraction of its width.
const ASPECT: f64 = 0.42;

fn cap_color(variant: Option<&str>) -> &'static str {
    match variant {
        Some("black") => "#2a2b2f",
        Some("grey") => "#b9bcc0",
        Some("cream") => "#e8e1cc",
        _ => "#f1f1ee",
    }
}

/// One key at (`x`, `y`) in `parent`, `w × h`, with its legend printed
/// `above` it; registered as a control of `mode`.
#[allow(clippy::too_many_arguments)]
fn key(
    b: &mut Builder<'_>,
    parent: NodeId,
    (x, y, w, h): (f64, f64, f64, f64),
    p: &Place,
    legend: &str,
    size: f64,
    mode: ButtonMode,
    name: &str,
) -> NodeId {
    let color = cap_color(p.variant.as_deref());
    let lab_h = if legend.is_empty() { 0.0 } else { size * 1.6 };
    let root = b.add(parent, draw::frame(name, x, y - lab_h, w, h + lab_h));
    let k = (w / 40.0).max(0.4);
    let cap = draw::rect(
        "cap",
        0.0,
        lab_h,
        w,
        h,
        lin(
            180.0,
            &[
                (0.0, tint(color, 0.5)),
                (0.18, color.to_string()),
                (0.8, shade(color, 0.06)),
                (1.0, shade(color, 0.22)),
            ],
        ),
    );
    let cap = draw::with_effects(
        draw::stroked(draw::rounded(cap, 1.2 * k), "#000000b0", k.clamp(0.6, 1.2)),
        vec![
            draw::drop_shadow("#000000a0", 0.0, 1.5 * k, 2.5 * k),
            draw::inner_shadow("#ffffff80", 0.0, k, 0.0),
        ],
    );
    b.add(root, cap);
    // In: the housing's shadow falls into the top of the cap.
    let pressed = draw::rect(
        "pressed",
        0.0,
        lab_h,
        w,
        h,
        lin(
            180.0,
            &[
                (0.0, "#000000b0".into()),
                (0.35, "#00000040".into()),
                (1.0, "#00000010".into()),
            ],
        ),
    );
    let pressed = b.add(
        root,
        draw::with_effects(
            draw::rounded(pressed, 1.2 * k),
            vec![draw::inner_shadow("#00000099", 0.0, 2.0 * k, 3.0 * k)],
        ),
    );
    let mut c = Control::new(ControlKind::Button { mode }).slot(SlotRole::Led, pressed.0);
    if !legend.is_empty() {
        let inkc = p.ink_color(PANEL_INK);
        let t = b.add(
            root,
            cased(
                ink(
                    "legend",
                    legend,
                    -w * 0.3,
                    size * 0.7,
                    w * 1.6,
                    size,
                    &inkc,
                    TextAlign::Center,
                    700.0,
                ),
                p.case(),
            ),
        );
        c = c.slot(SlotRole::Label, t.0);
    }
    c.binding.clone_from(&p.binding);
    b.controls.insert(root, c);
    root
}

/// A black housing the keys sit in.
fn housing(b: &mut Builder<'_>, parent: NodeId, (x, y, w, h): (f64, f64, f64, f64), k: f64) {
    let n = draw::rect(
        "housing",
        x,
        y,
        w,
        h,
        lin(180.0, &[(0.0, "#050506".into()), (1.0, "#16171a".into())]),
    );
    b.add(
        parent,
        draw::with_effects(
            draw::rounded(n, 1.5 * k),
            vec![
                draw::inner_shadow("#000000", 0.0, k, 2.0 * k),
                draw::drop_shadow("#ffffff1c", 0.0, k, 0.0),
            ],
        ),
    );
}

fn legend_size(p: &Place, w: f64) -> f64 {
    p.opt("legend")
        .and_then(|v| v.parse().ok())
        .unwrap_or((w * 0.2).clamp(6.5, 10.0))
}

fn piano(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let w = p.size.unwrap_or(44.0).max(20.0);
    let h = w * ASPECT;
    let pad = (w * 0.07).max(2.0);
    let legend = p.legend.clone().unwrap_or_default();
    let size = legend_size(p, w);
    let lab_h = if legend.is_empty() { 0.0 } else { size * 1.6 };
    let root = b.add(
        parent,
        draw::frame(&p.name, 0.0, 0.0, w + 2.0 * pad, h + 2.0 * pad + lab_h),
    );
    housing(
        b,
        root,
        (0.0, lab_h, w + 2.0 * pad, h + 2.0 * pad),
        w / 40.0,
    );
    let mode = if p.flag("momentary") {
        ButtonMode::Momentary
    } else {
        ButtonMode::Toggle
    };
    let k = key(
        b,
        root,
        (pad, pad + lab_h, w, h),
        p,
        &legend,
        size,
        mode,
        &format!("{} key", p.name),
    );
    // The control is the key; the outer frame only holds it.
    if let Some(c) = b.controls.remove(&k) {
        b.controls.insert(root, c);
    }
    root
}

fn piano_bank(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let w = p.size.unwrap_or(44.0).max(20.0);
    let h = w * ASPECT;
    let pad = (w * 0.07).max(2.0);
    let gap = (w * 0.05).max(1.5);
    // `key=value` entries configure the bank; the rest are its keys.
    let mut opts: Vec<String> = p
        .options
        .iter()
        .flatten()
        .filter(|o| !o.contains('='))
        .cloned()
        .collect();
    if opts.is_empty() {
        opts = (1..=3).map(|i| i.to_string()).collect();
    }
    let n = opts.len();
    let size = legend_size(p, w);
    let lab_h = size * 1.6;
    let total = n as f64 * w + (n as f64 - 1.0) * gap;
    let root = b.add(
        parent,
        draw::frame(&p.name, 0.0, 0.0, total + 2.0 * pad, h + 2.0 * pad + lab_h),
    );
    housing(
        b,
        root,
        (0.0, lab_h, total + 2.0 * pad, h + 2.0 * pad),
        w / 40.0,
    );
    for (i, label) in opts.iter().enumerate() {
        let at = if p.reverse { n - 1 - i } else { i };
        let x = pad + at as f64 * (w + gap);
        let mode = ButtonMode::Select {
            index: i as u32,
            count: n as u32,
        };
        key(
            b,
            root,
            (x, pad + lab_h, w, h),
            p,
            label,
            size,
            mode,
            &format!("{} {label}", p.name),
        );
    }
    root
}

/// This family's components.
pub(crate) fn defs() -> Vec<ComponentDef> {
    let def = |id, title, description, binds, build: BuildFn| ComponentDef {
        id,
        family: Family::Button,
        title,
        description,
        variants: &["white", "cream", "grey", "black"],
        default_size: 44.0,
        binds,
        build,
    };
    vec![
        def(
            "button.piano",
            "Piano key",
            "A wide, low, square-cornered white push key in a black housing, unlit, read by whether it is in — a 1970s desktop compressor's POWER key. `legend` prints on the panel above it. Size = cap width.",
            "toggle param",
            piano,
        ),
        def(
            "button.piano_bank",
            "Piano key bank",
            "Interlocked piano keys butted into one black housing, one per `Place::options` entry printed above it; the engaged one sits in — a desktop compressor's METER row (input / output / gain change). Each key is a `Select` button over a choice param. Size = cap width.",
            "choice param",
            piano_bank,
        ),
    ]
}

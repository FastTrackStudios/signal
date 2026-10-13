//! `button.push_in_key_bank`: interlocking push keys butted into one upright
//! column, drawn from photographs of the 1176's ratio and meter banks.
//!
//! Tall, nearly square-cornered plastic keys, each a little taller than two
//! widths, stacked with a hairline between them in a black slot. The engaged
//! key is pushed in — it sits back in the slot, darker, its top edge in
//! shadow — while the others stand proud, lit along their top edge and
//! throwing a small shadow down onto the key below. The positions are
//! printed on the panel beside the column, to its left (the ratio bank) or
//! right (the meter bank).
//!
//! Each key is a `Select` button over a choice param; `Led` = the pushed-in
//! overlay. `Place::options` are the positions top to bottom (reversed with
//! `Place::reverse`), plus settings: `labels=left|right|none`, and per-key
//! cap colours `colors=#a,#b,…` (a red OFF key). Size = key width.

use document_proto::{NodeId, TextAlign};
use surface_proto::{ButtonMode, Control, ControlKind, SlotRole};
use surface_widgets::Builder;

use super::parts::{PANEL_INK, alpha, cased, fixed, ink, lin, shade, text_w, tint};
use crate::draw;
use crate::registry::{ComponentDef, Family, Place};

/// Key height, in key widths (measured: 65 × 35 px).
const ASPECT: f64 = 1.85;

fn key_column(b: &mut Builder<'_>, parent: NodeId, p: &Place) -> NodeId {
    let w = p.size.unwrap_or(20.0).max(8.0);
    let k = w / 20.0;
    let h = w * p
        .opt("aspect")
        .and_then(|v| v.parse().ok())
        .unwrap_or(ASPECT);
    let base = match p.variant.as_deref() {
        Some("white") => "#e9e7e0",
        Some("grey") => "#8c8e91",
        _ => "#1b1b1d",
    };
    let panel_ink = p.ink_color(PANEL_INK);
    let opts: Vec<String> = p
        .options
        .iter()
        .flatten()
        .filter(|o| !o.contains('='))
        .cloned()
        .collect();
    let opts = if opts.is_empty() {
        vec!["1".into(), "2".into(), "3".into()]
    } else {
        opts
    };
    let n = opts.len();
    let side = p.opt("labels").unwrap_or("left");
    let size = (10.0 * k).max(8.0);
    let label_w = if side == "none" {
        0.0
    } else {
        opts.iter()
            .map(|o| text_w(o, size))
            .fold(size * 1.6, f64::max)
    };
    let gap = 7.0 * k;
    let pad = 1.5 * k;
    let (sw, sh) = (w + 2.0 * pad, n as f64 * h + 2.0 * pad);
    let slot_x = if side == "left" { label_w + gap } else { 0.0 };
    let total_w = sw + if side == "none" { 0.0 } else { label_w + gap };
    let root = b.add(parent, draw::frame(&p.name, 0.0, 0.0, total_w, sh));
    // The slot the keys stand in: a black opening with a hairline lip.
    let slot = draw::rect("slot", slot_x, 0.0, sw, sh, draw::solid("#050506"));
    let slot = draw::stroked(draw::rounded(slot, 1.5 * k), "#00000099", k.max(0.8));
    b.add(
        root,
        fixed(draw::with_effects(
            slot,
            vec![
                draw::drop_shadow("#ffffff1f", 0.0, k.max(0.8), 0.0),
                draw::inner_shadow("#000000", 0.0, k, 2.0 * k),
            ],
        )),
    );
    for (i, label) in opts.iter().enumerate() {
        let at = if p.reverse { n - 1 - i } else { i };
        let y = pad + at as f64 * h;
        let row = b.add(
            root,
            draw::frame(&format!("{} {label}", p.name), 0.0, y, total_w, h),
        );
        let (lx, align) = if side == "left" {
            (0.0, TextAlign::Right)
        } else {
            (sw + gap, TextAlign::Left)
        };
        let text = b.add(
            row,
            cased(
                ink(
                    "legend",
                    label,
                    lx,
                    h / 2.0,
                    label_w,
                    size,
                    &panel_ink,
                    align,
                    p.opt("weight")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(700.0),
                ),
                p.case(),
            ),
        );
        let cap_c = p.lamp_color(i).unwrap_or_else(|| base.to_string());
        let (x, cw, ch) = (slot_x + pad, w, h - 0.8 * k);
        // Standing proud: lit along the top, a shadow cast onto the next key.
        let up = draw::rect(
            "cap",
            x,
            0.0,
            cw,
            ch,
            lin(
                180.0,
                &[
                    (0.0, tint(&cap_c, 0.22)),
                    (0.08, tint(&cap_c, 0.08)),
                    (0.8, cap_c.clone()),
                    (1.0, shade(&cap_c, 0.3)),
                ],
            ),
        );
        let up = draw::with_effects(
            draw::rounded(up, 1.2 * k),
            vec![
                draw::drop_shadow("#000000b3", 0.0, 1.4 * k, 1.8 * k),
                draw::inner_shadow(&alpha("#ffffff", 0.22), 0.0, 0.9 * k, 0.0),
            ],
        );
        b.add(row, up);
        // Pushed in: back in the slot, darker, its top edge in shadow.
        let down = draw::rect(
            "pressed",
            x,
            0.0,
            cw,
            ch,
            lin(
                180.0,
                &[
                    (0.0, shade(&cap_c, 0.55)),
                    (0.25, shade(&cap_c, 0.25)),
                    (1.0, shade(&cap_c, 0.12)),
                ],
            ),
        );
        let down = draw::with_effects(
            draw::rounded(down, 1.2 * k),
            vec![draw::inner_shadow("#000000d9", 0.0, 2.2 * k, 3.0 * k)],
        );
        let down = b.add(row, down);
        let mut c = Control::new(ControlKind::Button {
            mode: ButtonMode::Select {
                index: i as u32,
                count: n as u32,
            },
        })
        .slot(SlotRole::Led, down.0)
        .slot(SlotRole::Label, text.0);
        c.binding.clone_from(&p.binding);
        b.controls.insert(row, c);
    }
    root
}

pub(crate) fn defs() -> Vec<ComponentDef> {
    vec![ComponentDef {
        id: "button.push_in_key_bank",
        family: Family::Button,
        title: "Key column (FET limiter)",
        description: "Interlocking push keys butted into one upright column in a black slot — the FET limiter's ratio and meter banks. The engaged key sits pushed in; the others stand proud. Positions are printed on the panel beside the column (`labels=left|right|none` in `options`); `colors=#a,#b,…` colours keys one by one (a red OFF key). A `Select` button per option over a choice param. Size = key width. Variants: black, white, grey keys.",
        variants: &["black", "white", "grey"],
        default_size: 20.0,
        binds: "choice param",
        build: key_column,
    }]
}

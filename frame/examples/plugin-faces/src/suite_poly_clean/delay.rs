//! `10-post-onyx-delay` — the big-box delay (BigBox class, 404 × 286): a
//! matte black floor unit. Three slot levers along the top (sync, mode,
//! type) feed a teal LCD that lists each lever's positions (the chosen one
//! bright) over the time, tempo and feedback readouts; mix and feedback
//! above, low cut, time, amount and high cut below; the engage and
//! tap-tempo stomps at the corners with their white LEDs, the name
//! between them.

use document_proto::{NodeId, NodeKind, TextAlign};
use surface_proto::{ButtonMode, Control, ControlKind, Range, SlotRole, Unit, ValueFormat};
use surface_widgets::Builder;

use super::parts::{self, BIG_W, BLACK, PEDAL_H, jacks, knob, pedal, switch, word};
use super::PLUGIN;
use crate::face::{Face, canvas};
use crate::reverb::ui;

const NS: &str = "delay";
const W: f64 = BIG_W;
const LH: f64 = PEDAL_H - parts::LIP;
/// Reference → design: the reference unit is 836 × 635 (lid).
const SX: f64 = W / 836.0;
const SY: f64 = LH / 635.0;
const INK: &str = "#f2f2f2";

const SYNC: [&str; 3] = ["TAP", "DAW", "FREE"];
const MODE: [&str; 3] = ["WIDE", "PING PONG", "NORMAL"];
const TYPE: [&str; 3] = ["DIGITAL", "DIFFUSION", "MODERN"];

fn px(x: f64) -> f64 {
    x * SX
}
fn py(y: f64) -> f64 {
    y * SY
}

/// One LCD column: a lever's positions, the chosen one lit (a `Select`
/// button per line — tap a line to choose it).
fn column(b: &mut Builder<'_>, lcd: NodeId, addr: &str, opts: &[&str], cx: f64, top: f64, w: f64) {
    let row = 9.5;
    for (i, o) in opts.iter().enumerate() {
        let cy = top + row * i as f64;
        let cell = b.add(lcd, surface_kit::draw::frame(&format!("{o} line"), cx - w / 2.0, cy - row / 2.0, w, row));
        b.add(cell, ui::text("dim", o, 0.0, row / 2.0, w, 6.8, "#d8efe999", TextAlign::Center, 500.0, false));
        let lit = b.add(cell, ui::glowing(ui::text("lit", o, 0.0, row / 2.0, w, 6.8, "#ffffff", TextAlign::Center, 800.0, false), "#e8fff9", 0.35, 2.0));
        let mut c = Control::new(ControlKind::Button { mode: ButtonMode::Select { index: i as u32, count: opts.len() as u32 } }).slot(SlotRole::Led, lit.0);
        c.binding = Some(ui::bind(addr));
        b.controls.insert(cell, c);
    }
}

pub fn face() -> Face {
    let scene = canvas("Onyx Delay (BigBox pedal, 404×286)", |b, root| {
        let lid = pedal(b, root, W, PEDAL_H, ["#303032", "#29292b", "#232325"], "#5a5a5e");
        // Top: the jacks' arrows and the three levers with their guide lines.
        jacks(b, lid, px(417.0), py(25.0), px(223.0), INK, false);
        for ((addr, label), x) in [("delay/sync", "sync"), ("delay/mode", "mode"), ("delay/type", "type")].iter().zip([257.0, 419.0, 573.0]) {
            word(b, lid, label, px(x), py(24.0), 10.5, INK, 500.0, 0.08);
            switch(b, lid, "switch.slot_lever", "", addr, label, 32.0, px(x), py(64.0), &[], &["knob"]);
            ui::line(b, lid, "guide", &[(px(x), py(80.0)), (px(x), py(106.0))], 0.9, INK);
            b.add(lid, surface_kit::draw::circle("guide dot", px(x), py(80.0), 1.6, surface_kit::draw::solid(INK)));
        }
        // The LCD.
        let (dx, dy, dw, dh) = (px(187.0), py(110.0), px(460.0), py(128.0));
        let bezel = surface_kit::draw::rounded(surface_kit::draw::rect("lcd bezel", dx - 3.0, dy - 3.0, dw + 6.0, dh + 6.0, surface_kit::draw::solid("#0c0c0d")), 4.0);
        b.add(lid, surface_kit::draw::with_effects(bezel, vec![surface_kit::draw::inner_shadow("#000000", 0.0, 1.0, 2.0), surface_kit::draw::drop_shadow("#ffffff26", 0.0, 1.0, 0.5)]));
        let mut face = surface_kit::draw::frame("LCD", dx, dy, dw, dh);
        face.fills = vec![
            surface_kit::draw::linear(170.0, &[(0.0, "#5f9990"), (0.5, "#4b837b"), (1.0, "#3c6f68")]),
            surface_kit::draw::radial_at(0.3, 0.4, 0.5, &[(0.0, "#ffffff14"), (1.0, "#ffffff00")]),
            surface_kit::draw::radial_at(0.75, 0.7, 0.4, &[(0.0, "#0000001a"), (1.0, "#00000000")]),
        ];
        face.clips_content = true;
        face.corner_radii = document_proto::CornerRadii::uniform(2.0);
        face.effects = vec![surface_kit::draw::inner_shadow("#0000008c", 0.0, 1.5, 3.0)];
        let lcd = b.add(lid, face);
        let lcd = b.add(lcd, surface_kit::draw::frame("backlight", 0.0, 0.0, dw, dh));
        if let Some(n) = b.scene.node_mut(lcd) {
            n.emission = Some(document_proto::Emission { color: surface_kit::draw::hex("#6fd0bf"), intensity: 0.15, radius: 10.0 });
        }
        let cw = dw / 3.0;
        column(b, lcd, "delay/sync", &SYNC, cw * 0.5, 10.0, cw - 6.0);
        column(b, lcd, "delay/mode", &MODE, cw * 1.5, 10.0, cw - 6.0);
        column(b, lcd, "delay/type", &TYPE, cw * 2.5, 10.0, cw - 6.0);
        for (i, addr) in ["delay/time", "delay/tempo", "delay/feedback"].iter().enumerate() {
            let r = ui::lit_readout(b, lcd, &format!("{addr} readout"), ui::bind(addr), cw * i as f64 + 3.0, dh * 0.8, cw - 6.0, 10.5, "#f6fffc", TextAlign::Center);
            if *addr == "delay/tempo"
                && let Some(c) = b.controls.get_mut(&r)
            {
                c.format = Some(ValueFormat::decimals(1).unit("BPM"));
            }
            if let Some(v) = b.controls.get(&r).and_then(|c| c.slots_of(SlotRole::Value).next()).map(NodeId)
                && let Some(NodeKind::Text(t)) = b.scene.node_mut(v).map(|n| &mut n.kind)
            {
                // The LCD's own dot-matrix characters.
                t.font_family = Some(surface_kit::draw::family::DOTO.into());
                t.font_weight = Some(800.0);
                t.font_size *= 1.25;
            }
        }
        // Lines from the LCD down to time and amount.
        for (x0, x1) in [(257.0, 290.0), (573.0, 548.0)] {
            let pts = [(px(x0), py(242.0)), (px(x0), py(318.0)), (px(x1), py(318.0))];
            ui::line(b, lid, "guide", &pts, 0.9, INK);
            b.add(lid, surface_kit::draw::circle("guide dot", px(x0), py(242.0), 1.6, surface_kit::draw::solid(INK)));
        }
        // Knobs.
        let k = 40.0;
        word(b, lid, "mix", px(103.0), py(80.0), 10.5, INK, 500.0, 0.08);
        word(b, lid, "feedback", px(731.0), py(80.0), 10.5, INK, 500.0, 0.08);
        knob(b, lid, &BLACK, "delay/mix", "Mix", k, px(106.0), py(150.0), "#2a1d0c");
        knob(b, lid, &BLACK, "delay/feedback", "Feedback", k, px(731.0), py(150.0), "#2a1d0c");
        knob(b, lid, &BLACK, "delay/lowcut", "Low Cut", k, px(106.0), py(300.0), "#2a1d0c");
        knob(b, lid, &BLACK, "delay/time", "Time", k, px(313.0), py(300.0), "#2a1d0c");
        knob(b, lid, &BLACK, "delay/amount", "Amount", k, px(525.0), py(300.0), "#2a1d0c");
        knob(b, lid, &BLACK, "delay/highcut", "High Cut", k, px(731.0), py(300.0), "#2a1d0c");
        parts::cut_glyph(b, lid, px(106.0), py(385.0), 14.0, true, INK);
        parts::cut_glyph(b, lid, px(731.0), py(385.0), 14.0, false, INK);
        word(b, lid, "time", px(313.0), py(392.0), 10.5, INK, 500.0, 0.08);
        word(b, lid, "amount", px(525.0), py(392.0), 10.5, INK, 500.0, 0.08);
        // Stomps, LEDs inboard, and the name.
        word(b, lid, "engaged", px(103.0), py(472.0), 10.0, INK, 500.0, 0.02);
        word(b, lid, "tap tempo", px(729.0), py(472.0), 10.0, INK, 500.0, 0.02);
        switch(b, lid, "switch.hex_stomp", "chrome", "delay/on", "Engage", 24.0, px(106.0), py(540.0), &["led=right"], &["cap"]);
        switch(b, lid, "switch.hex_stomp", "chrome", "delay/tap", "Tap Tempo", 24.0, px(731.0), py(540.0), &["led=left", "momentary"], &["cap"]);
        word(b, lid, "delay", px(419.0), py(530.0), 22.0, INK, 600.0, 0.1);
    });
    let params = vec![
        parts::pct(NS, "mix", "Mix", 55.0).featured(),
        parts::pct(NS, "feedback", "Feedback", 55.0).featured(),
        ui::param("delay/time", "Time", Range::log(20.0, 2000.0, Unit::Milliseconds), 375.0).featured(),
        parts::pct(NS, "amount", "Amount", 50.0),
        parts::hz(NS, "lowcut", "Low Cut", 20.0, 1000.0, 23.0),
        parts::hz(NS, "highcut", "High Cut", 1000.0, 20000.0, 17500.0),
        ui::param("delay/tempo", "Tempo", Range::linear(40.0, 240.0, Unit::None), 120.0),
        parts::pick(NS, "sync", "Sync", &["Tap", "DAW", "Free"], 0),
        parts::pick(NS, "mode", "Mode", &["Wide", "Ping Pong", "Normal"], 2),
        parts::pick(NS, "type", "Type", &["Digital", "Diffusion", "Modern"], 2),
        parts::on(NS, "on", "Engage", true).featured(),
        parts::on(NS, "tap", "Tap Tempo", false),
    ];
    Face { file: "10-post-onyx-delay", plugin: PLUGIN, scene, params, streams: vec![] }
}

//! The Standard-class pedals (156 × 286): three before the amp, two after.
//!
//! - `01-pre-gold-boost` — a champagne-gold boost: gain, level, bass,
//!   treble on gold knurled knobs.
//! - `02-pre-bronze-compressor` — a bronze compressor: comp and level, an
//!   attack lever (slow / fast).
//! - `03-pre-rose-overdrive` — a dusty-rose overdrive: drive, level, tone.
//! - `09-post-mint-chorus` — a mint chorus with one big pearl mix knob.
//! - `11-post-lilac-reverb` — a lilac reverb: mix, decay, low and high cut
//!   on magenta knobs, a shimmer lever.
//!
//! Each pedal's lettering is lower-case, geometric and tracked wide, the
//! jack glyphs printed along the top, the stomp low with its white LED to
//! the right.

use frame::document::proto::NodeId;
use frame::surface::widgets::Builder;

use super::PLUGIN;
use super::parts::{
    self, GOLD, MAGENTA, PEARL, PEDAL_H, ROSE, STD_W, jacks, knob, pedal, switch, word,
};
use crate::face::{Face, canvas};

const LH: f64 = PEDAL_H - parts::LIP;
const W: f64 = STD_W;

fn x(f: f64) -> f64 {
    W * f
}
fn y(f: f64) -> f64 {
    LH * f
}

/// The stomp and its LED.
fn stomp(b: &mut Builder<'_>, lid: NodeId, ns: &str, finish: &str) {
    switch(
        b,
        lid,
        "switch.hex_stomp",
        finish,
        &format!("{ns}/on"),
        "Bypass",
        27.0,
        x(0.5),
        y(0.83),
        &["led=right"],
        &["cap"],
    );
}

pub fn boost() -> Face {
    const NS: &str = "boost";
    let ink = "#fbf6ef";
    let scene = canvas("Gold Boost (Standard pedal, 156×286)", |b, root| {
        let lid = pedal(
            b,
            root,
            W,
            PEDAL_H,
            ["#c6b39c", "#ac9880", "#94806a"],
            "#f1e3cf",
        );
        jacks(b, lid, x(0.5), y(0.045), 26.0, ink, true);
        word(b, lid, "gain", x(0.24), y(0.088), 10.0, ink, 700.0, 0.12);
        word(b, lid, "level", x(0.76), y(0.088), 10.0, ink, 700.0, 0.12);
        knob(
            b,
            lid,
            &GOLD,
            "boost/gain",
            "Gain",
            36.0,
            x(0.24),
            y(0.19),
            "#2a1d0c",
        );
        knob(
            b,
            lid,
            &GOLD,
            "boost/level",
            "Level",
            36.0,
            x(0.76),
            y(0.19),
            "#2a1d0c",
        );
        knob(
            b,
            lid,
            &GOLD,
            "boost/bass",
            "Bass",
            36.0,
            x(0.24),
            y(0.385),
            "#2a1d0c",
        );
        knob(
            b,
            lid,
            &GOLD,
            "boost/treble",
            "Treble",
            36.0,
            x(0.76),
            y(0.385),
            "#2a1d0c",
        );
        word(b, lid, "bass", x(0.24), y(0.535), 10.0, ink, 700.0, 0.12);
        word(b, lid, "treble", x(0.76), y(0.535), 10.0, ink, 700.0, 0.12);
        word(b, lid, "boost", x(0.5), y(0.63), 22.0, ink, 700.0, 0.07);
        stomp(b, lid, NS, "gold");
    });
    let params = vec![
        parts::db(NS, "gain", "Gain", 0.0, 24.0, 8.5).featured(),
        parts::db(NS, "level", "Level", -12.0, 12.0, 0.0).bipolar(),
        parts::db(NS, "bass", "Bass", -12.0, 12.0, 0.0).bipolar(),
        parts::db(NS, "treble", "Treble", -12.0, 12.0, 0.0).bipolar(),
        parts::on(NS, "on", "Bypass", true).featured(),
    ];
    Face {
        file: "01-pre-gold-boost",
        plugin: PLUGIN,
        scene,
        params,
        streams: vec![],
    }
}

pub fn compressor() -> Face {
    const NS: &str = "comp";
    let ink = "#fbf4ea";
    let scene = canvas("Bronze Compressor (Standard pedal, 156×286)", |b, root| {
        let lid = pedal(
            b,
            root,
            W,
            PEDAL_H,
            ["#a07a4e", "#8a6538", "#76532b"],
            "#d9b88c",
        );
        jacks(b, lid, x(0.5), y(0.045), 26.0, ink, true);
        word(b, lid, "comp", x(0.22), y(0.088), 10.0, ink, 700.0, 0.12);
        word(b, lid, "level", x(0.78), y(0.088), 10.0, ink, 700.0, 0.12);
        knob(
            b,
            lid,
            &GOLD,
            "comp/comp",
            "Comp",
            36.0,
            x(0.22),
            y(0.19),
            "#2a1d0c",
        );
        knob(
            b,
            lid,
            &GOLD,
            "comp/level",
            "Level",
            36.0,
            x(0.78),
            y(0.19),
            "#2a1d0c",
        );
        switch(
            b,
            lid,
            "switch.mini_lever",
            "gold",
            "comp/attack",
            "Attack",
            22.0,
            x(0.5),
            y(0.42),
            &[],
            &["bushing"],
        );
        word(b, lid, "slow", x(0.345), y(0.375), 7.5, ink, 600.0, 0.04);
        word(b, lid, "fast", x(0.655), y(0.375), 7.5, ink, 600.0, 0.04);
        parts::cut_glyph(b, lid, x(0.36), y(0.405), 5.0, false, ink);
        word(b, lid, "attack", x(0.5), y(0.495), 10.0, ink, 700.0, 0.12);
        word(
            b,
            lid,
            "compressor",
            x(0.5),
            y(0.63),
            16.5,
            ink,
            700.0,
            0.07,
        );
        stomp(b, lid, NS, "gold");
    });
    let params = vec![
        parts::pct(NS, "comp", "Comp", 40.0).featured(),
        parts::db(NS, "level", "Level", -12.0, 12.0, 0.0).bipolar(),
        parts::on(NS, "attack", "Attack Fast", false),
        parts::on(NS, "on", "Bypass", true).featured(),
    ];
    Face {
        file: "02-pre-bronze-compressor",
        plugin: PLUGIN,
        scene,
        params,
        streams: vec![],
    }
}

pub fn overdrive() -> Face {
    const NS: &str = "drive";
    let ink = "#fbf3f5";
    let scene = canvas("Rose Overdrive (Standard pedal, 156×286)", |b, root| {
        let lid = pedal(
            b,
            root,
            W,
            PEDAL_H,
            ["#c09aa7", "#a67b8b", "#916676"],
            "#ecd2da",
        );
        jacks(b, lid, x(0.5), y(0.045), 26.0, ink, true);
        word(b, lid, "drive", x(0.23), y(0.088), 10.0, ink, 700.0, 0.12);
        word(b, lid, "level", x(0.77), y(0.088), 10.0, ink, 700.0, 0.12);
        knob(
            b,
            lid,
            &ROSE,
            "drive/drive",
            "Drive",
            36.0,
            x(0.23),
            y(0.19),
            "#2a1d0c",
        );
        knob(
            b,
            lid,
            &ROSE,
            "drive/level",
            "Level",
            36.0,
            x(0.77),
            y(0.19),
            "#2a1d0c",
        );
        knob(
            b,
            lid,
            &ROSE,
            "drive/tone",
            "Tone",
            36.0,
            x(0.5),
            y(0.36),
            "#2a1d0c",
        );
        word(b, lid, "tone", x(0.5), y(0.52), 10.0, ink, 700.0, 0.12);
        word(
            b,
            lid,
            "overdrive",
            x(0.5),
            y(0.635),
            18.0,
            ink,
            700.0,
            0.07,
        );
        stomp(b, lid, NS, "gold");
    });
    let params = vec![
        parts::pct(NS, "drive", "Drive", 40.0).featured(),
        parts::db(NS, "level", "Level", -12.0, 12.0, 0.0).bipolar(),
        parts::pct(NS, "tone", "Tone", 50.0),
        parts::on(NS, "on", "Bypass", true).featured(),
    ];
    Face {
        file: "03-pre-rose-overdrive",
        plugin: PLUGIN,
        scene,
        params,
        streams: vec![],
    }
}

pub fn chorus() -> Face {
    const NS: &str = "chorus";
    let ink = "#3f4745";
    let scene = canvas("Mint Chorus (Standard pedal, 156×286)", |b, root| {
        let lid = pedal(
            b,
            root,
            W,
            PEDAL_H,
            ["#d5e2de", "#c5d4d0", "#b3c4bf"],
            "#f4fbf9",
        );
        jacks(b, lid, x(0.5), y(0.045), 26.0, ink, true);
        word(b, lid, "mix", x(0.5), y(0.11), 10.0, ink, 500.0, 0.12);
        knob(
            b,
            lid,
            &PEARL,
            "chorus/mix",
            "Mix",
            58.0,
            x(0.5),
            y(0.25),
            "#2a1d0c",
        );
        word(b, lid, "chorus", x(0.5), y(0.545), 22.0, ink, 500.0, 0.12);
        switch(
            b,
            lid,
            "switch.hex_stomp",
            "chrome",
            "chorus/on",
            "Bypass",
            27.0,
            x(0.5),
            y(0.83),
            &["led=right"],
            &["cap"],
        );
    });
    let params = vec![
        parts::pct(NS, "mix", "Mix", 42.0).featured(),
        parts::on(NS, "on", "Bypass", true).featured(),
    ];
    Face {
        file: "09-post-mint-chorus",
        plugin: PLUGIN,
        scene,
        params,
        streams: vec![],
    }
}

pub fn reverb() -> Face {
    const NS: &str = "verb";
    let ink = "#8e6f7d";
    let scene = canvas("Lilac Reverb (Standard pedal, 156×286)", |b, root| {
        let lid = pedal(
            b,
            root,
            W,
            PEDAL_H,
            ["#efe2ec", "#e3d2df", "#d5c1d0"],
            "#fff8fd",
        );
        jacks(b, lid, x(0.5), y(0.045), 26.0, "#3b2f36", true);
        word(b, lid, "mix", x(0.23), y(0.085), 10.0, ink, 700.0, 0.1);
        word(b, lid, "decay", x(0.77), y(0.085), 10.0, ink, 500.0, 0.12);
        knob(
            b,
            lid,
            &MAGENTA,
            "verb/mix",
            "Mix",
            36.0,
            x(0.23),
            y(0.195),
            "#2a1d0c",
        );
        knob(
            b,
            lid,
            &MAGENTA,
            "verb/decay",
            "Decay",
            36.0,
            x(0.77),
            y(0.195),
            "#2a1d0c",
        );
        knob(
            b,
            lid,
            &MAGENTA,
            "verb/low",
            "Low Cut",
            36.0,
            x(0.23),
            y(0.37),
            "#2a1d0c",
        );
        knob(
            b,
            lid,
            &MAGENTA,
            "verb/high",
            "High Cut",
            36.0,
            x(0.77),
            y(0.37),
            "#2a1d0c",
        );
        switch(
            b,
            lid,
            "switch.mini_lever",
            "magenta",
            "verb/shimmer",
            "Shimmer",
            16.0,
            x(0.5),
            y(0.3),
            &[],
            &["bushing"],
        );
        word(b, lid, "off", x(0.43), y(0.21), 7.0, "#3b2f36", 500.0, 0.04);
        word(b, lid, "on", x(0.585), y(0.21), 7.0, "#3b2f36", 500.0, 0.04);
        word(
            b,
            lid,
            "shimmer",
            x(0.5),
            y(0.39),
            7.5,
            "#3b2f36",
            600.0,
            0.04,
        );
        parts::cut_glyph(b, lid, x(0.23), y(0.49), 14.0, true, ink);
        parts::cut_glyph(b, lid, x(0.77), y(0.49), 14.0, false, ink);
        word(b, lid, "reverb", x(0.5), y(0.575), 22.0, ink, 700.0, 0.1);
        switch(
            b,
            lid,
            "switch.hex_stomp",
            "magenta",
            "verb/on",
            "Bypass",
            27.0,
            x(0.5),
            y(0.83),
            &["led=right"],
            &["cap"],
        );
    });
    let params = vec![
        parts::pct(NS, "mix", "Mix", 8.0).featured(),
        parts::pct(NS, "decay", "Decay", 58.0).featured(),
        parts::hz(NS, "low", "Low Cut", 20.0, 1000.0, 24.0),
        parts::hz(NS, "high", "High Cut", 1000.0, 20000.0, 17000.0),
        parts::on(NS, "shimmer", "Shimmer", false),
        parts::on(NS, "on", "Bypass", true).featured(),
    ];
    Face {
        file: "11-post-lilac-reverb",
        plugin: PLUGIN,
        scene,
        params,
        streams: vec![],
    }
}

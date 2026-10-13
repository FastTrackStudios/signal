//! The families frame ships, embedded. All are SIL Open Font License 1.1;
//! each family's `OFL.txt` sits beside its fonts in `assets/`.
//!
//! Fonts without a Reserved Font Name are subset to Latin-1 plus the
//! punctuation, arrows, maths and music signs surfaces print (see
//! `assets/README.md`); fonts with one ship as released.

/// frame's UI face (Inter, variable `opsz`/`wght`).
pub static INTER: &[u8] = include_bytes!("../assets/inter/Inter-Variable.ttf");
/// Inter's italic.
pub static INTER_ITALIC: &[u8] = include_bytes!("../assets/inter/Inter-Italic-Variable.ttf");

/// A bundled family: its name, what it is for, and its font files.
#[derive(Debug, Clone, Copy)]
pub struct BundledFamily {
    /// The family name a `<Text font="…">` uses.
    pub name: &'static str,
    /// What kind of face it is, and what it suits.
    pub role: &'static str,
    /// Weights it draws: `(min, max)`.
    pub weights: (u16, u16),
    /// Widths (percent) it draws.
    pub widths: (u16, u16),
    /// Has an italic of its own (or an italic axis).
    pub italic: bool,
    pub files: &'static [&'static [u8]],
}

macro_rules! font {
    ($path:literal) => {
        include_bytes!(concat!("../assets/", $path)) as &[u8]
    };
}

/// Every bundled family, UI face first.
pub static BUNDLED: &[BundledFamily] = &[
    BundledFamily {
        name: "Inter",
        role: "UI sans (default): labels, values, legends",
        weights: (100, 900),
        widths: (100, 100),
        italic: true,
        files: &[INTER, INTER_ITALIC],
    },
    BundledFamily {
        name: "Oswald",
        role: "condensed display sans: model names, big legends, amp badges",
        weights: (200, 700),
        widths: (100, 100),
        italic: false,
        files: &[font!("oswald/Oswald-Variable.ttf")],
    },
    BundledFamily {
        name: "Saira",
        role: "technical sans with a width axis (50–125): condensed to extended rack lettering",
        weights: (100, 900),
        widths: (50, 125),
        italic: false,
        files: &[font!("saira/Saira-Variable.ttf")],
    },
    BundledFamily {
        name: "Michroma",
        role: "wide engraved/geometric caps: brushed-metal panels, hi-fi faceplates",
        weights: (400, 400),
        widths: (100, 100),
        italic: false,
        files: &[font!("michroma/Michroma-Regular.ttf")],
    },
    BundledFamily {
        name: "Orbitron",
        role: "geometric tech caps: synth and modern-metal logos",
        weights: (400, 900),
        widths: (100, 100),
        italic: false,
        files: &[font!("orbitron/Orbitron-Variable.ttf")],
    },
    BundledFamily {
        name: "Dancing Script",
        role: "flowing script: signature logos, boutique and vintage badges",
        weights: (400, 700),
        widths: (100, 100),
        italic: false,
        files: &[font!("dancing-script/DancingScript-Variable.ttf")],
    },
    BundledFamily {
        name: "Pacifico",
        role: "fat retro brush script: surf, funk and 50s logos",
        weights: (400, 400),
        widths: (100, 100),
        italic: false,
        files: &[font!("pacifico/Pacifico-Regular.ttf")],
    },
    BundledFamily {
        name: "Caveat Brush",
        role: "marker / handwriting: scribbled labels, tape strips, notes",
        weights: (400, 400),
        widths: (100, 100),
        italic: false,
        files: &[font!("caveat-brush/CaveatBrush-Regular.ttf")],
    },
    BundledFamily {
        name: "Knewave",
        role: "heavy brush marker: punk, funk and graffiti logos",
        weights: (400, 400),
        widths: (100, 100),
        italic: false,
        files: &[font!("knewave/Knewave-Regular.ttf")],
    },
    BundledFamily {
        name: "Playfair Display",
        role: "high-contrast serif: boutique, hi-fi and valve-era script plates",
        weights: (400, 900),
        widths: (100, 100),
        italic: false,
        files: &[font!("playfair-display/PlayfairDisplay-Variable.ttf")],
    },
    BundledFamily {
        name: "DSEG7 Classic",
        role: "7-segment LCD/LED digits: tuners, BPM and time readouts (digits, a few letters)",
        weights: (300, 700),
        widths: (100, 100),
        italic: true,
        files: &[
            font!("dseg/DSEG7Classic-Light.ttf"),
            font!("dseg/DSEG7Classic-Regular.ttf"),
            font!("dseg/DSEG7Classic-Bold.ttf"),
            font!("dseg/DSEG7Classic-Italic.ttf"),
            font!("dseg/DSEG7Classic-BoldItalic.ttf"),
        ],
    },
    BundledFamily {
        name: "DSEG14 Classic",
        role: "14-segment alphanumeric LCD: preset names, patch displays",
        weights: (400, 700),
        widths: (100, 100),
        italic: true,
        files: &[
            font!("dseg/DSEG14Classic-Regular.ttf"),
            font!("dseg/DSEG14Classic-Bold.ttf"),
            font!("dseg/DSEG14Classic-Italic.ttf"),
        ],
    },
    BundledFamily {
        name: "Doto",
        role: "dot-matrix (variable weight): VFD and LED-matrix displays",
        weights: (100, 900),
        widths: (100, 100),
        italic: false,
        files: &[font!("doto/Doto-Variable.ttf")],
    },
    BundledFamily {
        name: "VT323",
        role: "pixel terminal: CRT readouts, retro digital units",
        weights: (400, 400),
        widths: (100, 100),
        italic: false,
        files: &[font!("vt323/VT323-Regular.ttf")],
    },
    BundledFamily {
        name: "JetBrains Mono",
        role: "monospace: numeric readouts that must not jitter, code, tables",
        weights: (100, 800),
        widths: (100, 100),
        italic: false,
        files: &[font!("jetbrains-mono/JetBrainsMono-Variable.ttf")],
    },
    BundledFamily {
        name: "UnifrakturCook",
        role: "blackletter: metal and gothic logos",
        weights: (700, 700),
        widths: (100, 100),
        italic: false,
        files: &[font!("unifraktur-cook/UnifrakturCook-Bold.ttf")],
    },
    BundledFamily {
        name: "Pirata One",
        role: "condensed gothic/blackletter display: metal and pirate-flag logos",
        weights: (400, 400),
        widths: (100, 100),
        italic: false,
        files: &[font!("pirata-one/PirataOne-Regular.ttf")],
    },
];

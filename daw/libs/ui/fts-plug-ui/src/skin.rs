//! Per-plugin color identity.
//!
//! Every editor in the suite shares one layout language; the [`Skin`] is what
//! makes an EQ read as an EQ and a limiter as a limiter. It is a plain `Copy`
//! struct of CSS color strings rather than a theme-token lookup because the
//! values also have to reach inline `style="…"` attributes — Blitz does not
//! load external stylesheets reliably, so the whole suite styles inline.

/// Colors for one plugin's chrome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Skin {
    /// Section headings, selected segments, active toggles.
    pub accent: &'static str,
    /// Body text on panels.
    pub text: &'static str,
    /// Panel outlines.
    pub border: &'static str,
    /// Panel fill.
    pub panel: &'static str,
}

impl Skin {
    /// The neutral slate identity — the default for a plugin that has not
    /// picked its own colors yet.
    pub const NEUTRAL: Skin = Skin {
        accent: "#8aa4ff",
        text: "#f2f4f8",
        border: "rgba(148,163,184,0.30)",
        panel: "rgba(255,255,255,0.06)",
    };

    /// Build a skin from an accent, keeping the neutral text/panel treatment.
    ///
    /// `border` and `panel` stay neutral on purpose: tinting the fills as well
    /// as the accent makes the suite look like a set of unrelated plugins.
    pub const fn accented(accent: &'static str) -> Skin {
        Skin {
            accent,
            ..Skin::NEUTRAL
        }
    }
}

impl Default for Skin {
    fn default() -> Self {
        Skin::NEUTRAL
    }
}

/// Accents for the suite, one per plugin family, kept together so they can be
/// checked for distinctness at a glance.
pub mod accents {
    /// Dynamics — gate, expander.
    pub const GATE: &str = "#8ab4f8";
    /// Dynamics — limiter, ceiling stages.
    pub const LIMITER: &str = "#f28b82";
    /// Level riding / de-essing.
    pub const LEVEL: &str = "#81c995";
    /// Saturation / drive.
    pub const SATURATE: &str = "#fbbc04";
    /// Time — delay.
    pub const DELAY: &str = "#c58af9";
    /// Time — reverb.
    pub const REVERB: &str = "#78d9ec";
    /// Modulation — chorus, tremolo, wah.
    pub const MODULATION: &str = "#ff8bcb";
    /// Pitch — tune, pitch shift, unison.
    pub const PITCH: &str = "#f6a75c";
    /// Amp / cabinet modelling.
    pub const NAM: &str = "#a8c7fa";
}

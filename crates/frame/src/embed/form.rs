//! The `Embed` form factor: a module drawn *inside* a host's strip.
//!
//! Every other form (docs/form-factors.md §3.1) is a shape the module
//! chooses: a pedal is 156 × 286, a rack unit is 1140 × 103. An embed is a
//! shape the **host** chooses. A mixer strip gives it the strip's width and
//! whatever height its rack can spare; REAPER gives it a slot in the TCP or
//! MCP and resizes it with the track. So an embed never letterboxes: it
//! reflows, and what it shows is decided by its width, in tiers.
//!
//! The tiers are session's (`apps/session-daw/src/tone.rs`, `Rack::at`),
//! number for number, so a frame panel and the rack it replaces shed detail
//! at the same widths — a mixer of both reads as one mixer.
//!
//! These types are pure data with no dependency beyond serde, written to
//! move into `surface-proto::form` once the full form-factor system lands
//! (docs/form-factors.md §5.1): `FormFactor::Embed(EmbedForm)`.

use serde::{Deserialize, Serialize};

/// Where the embed is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EmbedHost {
    /// Our own mixer's channel strip (session's MCP): the Tone rack column,
    /// one strip wide, a unit per row.
    Strip,
    /// REAPER's track control panel: a short, wide slot beside the track
    /// name (22–80 px tall).
    Tcp,
    /// REAPER's mixer control panel: a slot in the strip's FX area, one
    /// strip wide (60–200 px), height from the strip.
    Mcp,
}

impl EmbedHost {
    pub const ALL: [Self; 3] = [Self::Strip, Self::Tcp, Self::Mcp];

    /// The page an authored document keeps this host's view on (see
    /// [`page_name`]).
    #[must_use]
    pub const fn page(self) -> &'static str {
        match self {
            Self::Strip => "embed:strip",
            Self::Tcp => "embed:tcp",
            Self::Mcp => "embed:mcp",
        }
    }

    /// The markup spelling, `form="embed:mcp"` (docs/form-factors.md §4.2).
    #[must_use]
    pub const fn markup(self) -> &'static str {
        self.page()
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|h| h.page() == s || h.page().trim_start_matches("embed:") == s)
    }

    /// What this host asks for when it sizes an embed: the numbers REAPER's
    /// `REAPER_FXEMBED_WM_GETMINMAXINFO` wants, and what a strip lays out.
    #[must_use]
    pub const fn hints(self) -> SizeHints {
        match self {
            // A strip column: the width is the strip's, the height is the
            // chain's, so only floors.
            Self::Strip => SizeHints {
                preferred_aspect: 0.0,
                minimum_aspect: 0.0,
                min: (MINIMAL, 24.0),
                max: (600.0, 4000.0),
            },
            // Wide and short, beside the name.
            Self::Tcp => SizeHints {
                preferred_aspect: 6.0,
                minimum_aspect: 1.5,
                min: (60.0, 22.0),
                max: (1200.0, 160.0),
            },
            // Tall and narrow, in the strip.
            Self::Mcp => SizeHints {
                preferred_aspect: 0.5,
                minimum_aspect: 0.1,
                min: (MINIMAL, 24.0),
                max: (600.0, 800.0),
            },
        }
    }
}

/// `FormFactor::Embed` — its only variant is the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EmbedForm {
    pub host: EmbedHost,
}

impl EmbedForm {
    #[must_use]
    pub const fn new(host: EmbedHost) -> Self {
        Self { host }
    }

    /// An embed always reflows: the host owns the size.
    #[must_use]
    pub const fn fit(self) -> EmbedFit {
        EmbedFit::Reflow
    }
}

/// How an embed takes its host's box. Always [`EmbedFit::Reflow`]; the
/// enum exists so `Fit` in `surface-proto::form` can absorb it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmbedFit {
    Reflow,
}

/// Size hints, in logical px. `0.0` means "no preference".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SizeHints {
    /// width / height.
    pub preferred_aspect: f64,
    pub minimum_aspect: f64,
    pub min: (f64, f64),
    pub max: (f64, f64),
}

impl SizeHints {
    /// Clamp a host's offer.
    #[must_use]
    pub fn clamp(&self, w: f64, h: f64) -> (f64, f64) {
        (
            w.clamp(self.min.0, self.max.0.max(self.min.0)),
            h.clamp(self.min.1, self.max.1.max(self.min.1)),
        )
    }
}

// ── tiers ────────────────────────────────────────────────────────────────

/// The narrowest width whose EQ still reads as a frequency axis
/// (session `tone::LEGIBLE`).
pub const LEGIBLE: f64 = 96.0;
/// The narrowest width that still draws a curve (session `tone::SHAPE`):
/// REAPER's 86 px strip, 82 once the insets come off, draws curves.
pub const SHAPE: f64 = 80.0;
/// The narrowest width that still draws indicators (session
/// `tone::MINIMAL`): a rail.
pub const MINIMAL: f64 = 16.0;
/// The narrowest width that hands the panel to the plugin's own editing
/// surface (session `tone::FOCUSED`).
pub const FOCUSED: f64 = 260.0;
/// What a selected strip opens to (session `tone::WORKING`).
pub const WORKING: f64 = 133.0;
/// REAPER's own strip, and session's default (`mcp::STRIP_W`).
pub const STRIP_W: f64 = 86.0;

/// Level of detail at a width — session's `Rack`, most capable first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tier {
    /// The plugin's own editing surface: grid, labels, band nodes.
    Focus,
    /// Every panel with its grid and its label.
    Full,
    /// The curves alone: no grid, no labels.
    Curves,
    /// A rail: one live indicator per unit.
    Minimal,
    /// Nothing; the host keeps the room.
    Off,
}

impl Tier {
    pub const ALL: [Self; 5] = [
        Self::Focus,
        Self::Full,
        Self::Curves,
        Self::Minimal,
        Self::Off,
    ];

    /// What fits in `width` (session's `Rack::at`, same thresholds).
    #[must_use]
    pub fn at(width: f64) -> Self {
        if width >= FOCUSED {
            Self::Focus
        } else if width >= LEGIBLE {
            Self::Full
        } else if width >= SHAPE {
            Self::Curves
        } else if width >= MINIMAL {
            Self::Minimal
        } else {
            Self::Off
        }
    }

    #[must_use]
    pub const fn on(self) -> bool {
        !matches!(self, Self::Off)
    }

    /// The narrowest width still in this tier.
    #[must_use]
    pub const fn floor(self) -> Option<f64> {
        match self {
            Self::Focus => Some(FOCUSED),
            Self::Full => Some(LEGIBLE),
            Self::Curves => Some(SHAPE),
            Self::Minimal => Some(MINIMAL),
            Self::Off => None,
        }
    }

    /// Headers, markers, grips.
    #[must_use]
    pub const fn detailed(self) -> bool {
        matches!(self, Self::Focus | Self::Full)
    }

    /// The plugin's own calibration (grid labels, nodes).
    #[must_use]
    pub const fn editing(self) -> bool {
        matches!(self, Self::Focus)
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Focus => "focus",
            Self::Full => "full",
            Self::Curves => "curves",
            Self::Minimal => "minimal",
            Self::Off => "off",
        }
    }
}

/// An authored embed page's name for `host` at a breakpoint:
/// `embed:mcp`, or `embed:mcp@96` for the view shown from 96 px wide up.
///
/// Views are pages (docs/form-factors.md §4.2, decided); until `Scene.views`
/// exists the page *name* carries the form. A document may hold several
/// pages per host, one per breakpoint; [`crate::embed::DocumentView`] picks the
/// widest breakpoint that fits and scales the page's width to the host's.
#[must_use]
pub fn page_name(host: EmbedHost, min_width: Option<u32>) -> String {
    match min_width {
        Some(w) => format!("{}@{w}", host.page()),
        None => host.page().to_string(),
    }
}

/// Parse a page name back: `(host, breakpoint)`.
#[must_use]
pub fn parse_page_name(name: &str) -> Option<(EmbedHost, u32)> {
    let (base, bp) = match name.split_once('@') {
        Some((b, w)) => (b, w.trim().parse().ok()?),
        None => (name, 0),
    };
    EmbedHost::ALL
        .into_iter()
        .find(|h| h.page() == base)
        .map(|h| (h, bp))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_are_sessions() {
        assert_eq!(Tier::at(300.0), Tier::Focus);
        assert_eq!(Tier::at(FOCUSED), Tier::Focus);
        assert_eq!(Tier::at(WORKING), Tier::Full);
        assert_eq!(Tier::at(LEGIBLE), Tier::Full);
        assert_eq!(
            Tier::at(STRIP_W),
            Tier::Curves,
            "REAPER's own strip draws curves"
        );
        assert_eq!(Tier::at(SHAPE - 0.5), Tier::Minimal);
        assert_eq!(Tier::at(MINIMAL), Tier::Minimal);
        assert_eq!(Tier::at(10.0), Tier::Off);
        for t in Tier::ALL {
            if let Some(f) = t.floor() {
                assert_eq!(Tier::at(f), t, "{t:?} starts at its floor");
            }
        }
    }

    #[test]
    fn page_names_round_trip() {
        assert_eq!(page_name(EmbedHost::Mcp, None), "embed:mcp");
        assert_eq!(parse_page_name("embed:mcp@96"), Some((EmbedHost::Mcp, 96)));
        assert_eq!(parse_page_name("embed:tcp"), Some((EmbedHost::Tcp, 0)));
        assert_eq!(parse_page_name("Page 1"), None);
        assert_eq!(EmbedHost::parse("strip"), Some(EmbedHost::Strip));
    }

    #[test]
    fn hints_clamp() {
        let h = EmbedHost::Tcp.hints();
        assert_eq!(h.clamp(20.0, 300.0), (60.0, 160.0));
    }
}

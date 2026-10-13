//! Token-based theme model (Phase 1 of the themeable-UI plan).
//!
//! Vector-first: a [`Theme`] is **owned, cloneable data** — a semantic
//! [`Tokens`] palette plus metrics — from which components resolve typed
//! per-element styles by `(role, state)`. No images required (an optional
//! image-skin layer comes later). Modelled on REAPER/WALTER's separation of
//! element identity ([`ElementRole`]) from appearance, but with computed
//! semantic tokens instead of hand-painted slots.
//!
//! This sits alongside the legacy `StyleSheet` (knob/slider/xypad) during the
//! migration; new panels consume [`Theme`] via [`super::context::use_theme`].

use super::mcp::McpTheme;
use super::style::ControlState;

// ─────────────────────────────────────────────────────────────────────────────
// Color — the unit of theming. RGBA u8 with the derivation math themes need.
// ─────────────────────────────────────────────────────────────────────────────

/// An RGBA color. Components render via [`Color::css`]; themes derive new
/// colors via [`Color::mix`] / [`Color::with_alpha`] / [`Color::on`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// Sentinel meaning "the track's colour" — theme importers bake layouts
    /// with this RGB and renderers substitute the live track accent
    /// (preserving the baked alpha). See [`Color::resolve_track`].
    pub const TRACK: Color = Color::rgba(252, 3, 244, 255);

    /// Substitute the track accent when this is the [`Color::TRACK`]
    /// sentinel (alpha carried over); otherwise unchanged.
    pub fn resolve_track(self, accent: Color) -> Color {
        if self.r == Self::TRACK.r && self.g == Self::TRACK.g && self.b == Self::TRACK.b {
            accent.with_alpha(self.a)
        } else {
            self
        }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parse `#rgb` / `#rrggbb` / `#rrggbbaa`. Returns `None` if malformed.
    pub fn hex(s: &str) -> Option<Self> {
        let h = s.strip_prefix('#')?;
        let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
        match h.len() {
            3 => {
                let d = |i: usize| u8::from_str_radix(&h[i..i + 1], 16).ok().map(|v| v * 17);
                Some(Self::rgb(d(0)?, d(1)?, d(2)?))
            }
            6 => Some(Self::rgb(p(0)?, p(2)?, p(4)?)),
            8 => Some(Self::rgba(p(0)?, p(2)?, p(4)?, p(6)?)),
            _ => None,
        }
    }

    /// CSS string (`rgba(r,g,b,a)`), ready for an inline `style`.
    pub fn css(&self) -> String {
        if self.a == 255 {
            format!("rgb({},{},{})", self.r, self.g, self.b)
        } else {
            format!(
                "rgba({},{},{},{:.3})",
                self.r,
                self.g,
                self.b,
                self.a as f32 / 255.0
            )
        }
    }

    /// Same color at a new alpha (0–255).
    pub fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// Linear blend toward `other` by `t` (0.0 = self, 1.0 = other). Alpha mixes too.
    pub fn mix(self, other: Color, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Self {
            r: lerp(self.r, other.r),
            g: lerp(self.g, other.g),
            b: lerp(self.b, other.b),
            a: lerp(self.a, other.a),
        }
    }

    /// Blend toward white / black by `t`.
    pub fn lighten(self, t: f32) -> Self {
        self.mix(Color::rgb(255, 255, 255), t)
    }
    pub fn darken(self, t: f32) -> Self {
        self.mix(Color::rgb(0, 0, 0), t)
    }

    /// Relative luminance (0–1), for contrast decisions.
    pub fn luminance(&self) -> f32 {
        (0.299 * self.r as f32 + 0.587 * self.g as f32 + 0.114 * self.b as f32) / 255.0
    }

    /// A legible near-black or near-white text color to sit *on* this color.
    pub fn on(&self) -> Color {
        if self.luminance() > 0.6 {
            Color::rgb(12, 12, 15)
        } else {
            Color::rgb(244, 244, 245)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Element identity + state (the stable theme/converter vocabulary).
// ─────────────────────────────────────────────────────────────────────────────

/// Stable themeable element identity — the id a theme targets and a
/// REAPER-theme importer maps onto (our analog of `tcp.volume`, `mcp.mute`, …).
/// Used by the (future) image-skin + converter layers; typed style getters on
/// [`Theme`] are the rendering path.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum ElementRole {
    PanelSurface,
    PanelHeader,
    Strip,
    StripFooter,
    TcpRow,
    Meter,
    Fader,
    Knob,
    MuteButton,
    SoloButton,
    RecArmButton,
    RouteButton,
    Label,
    TrackIndex,
    Lane,
    Clip,
    Ruler,
    FolderHeader,
}

/// Interaction + DAW state a component passes to the theme to resolve a style.
/// Extends the legacy [`ControlState`] with the DAW signals WALTER exposes as
/// scalars (`recarm`, `folderstate`, track colour, selection).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ThemeState {
    pub interaction: ControlState,
    pub selected: bool,
    pub armed: bool,
    pub soloed: bool,
    pub muted: bool,
    /// Source track colour for tinting (`None` = use the neutral token).
    pub track_color: Option<Color>,
    /// Folder nesting depth (0 = top level).
    pub folder_depth: u32,
}

impl ThemeState {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn track(mut self, color: Option<Color>) -> Self {
        self.track_color = color;
        self
    }
    pub fn selected(mut self, v: bool) -> Self {
        self.selected = v;
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Semantic tokens + metrics.
// ─────────────────────────────────────────────────────────────────────────────

/// Semantic colour roles — the layer components and per-element styles read
/// from (vs. REAPER's flat sea of named slots). Presets set these; params
/// (Phase 2) will derive some of them.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Tokens {
    pub surface: Color,
    pub surface_raised: Color,
    pub surface_sunken: Color,
    pub border: Color,
    pub text: Color,
    pub text_dim: Color,
    pub text_faint: Color,
    pub accent: Color,
    pub neutral_track: Color,
    pub meter_safe: Color,
    pub meter_warn: Color,
    pub meter_danger: Color,
    pub solo: Color,
    pub mute: Color,
    pub rec: Color,
    pub route_parent: Color,
    pub route_send: Color,
    pub route_recv: Color,
}

/// Non-colour metrics (radii/strokes/tint strengths). Phase-2 params feed these.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Metrics {
    pub radius: f32,
    /// Track-tint strength (0–1): how far a panel surface blends toward the
    /// track colour. The improvement over REAPER's manual `tinttcp`.
    pub track_tint: f32,
    /// Extra dim applied to unselected surfaces (0–1).
    pub dim_unselected: f32,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            radius: 5.0,
            track_tint: 0.12,
            dim_unselected: 0.0,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-element resolved styles (what components actually render).
// ─────────────────────────────────────────────────────────────────────────────

/// Panel chrome (surface + header + border + label text).
#[derive(Clone, PartialEq)]
pub struct PanelStyle {
    pub surface: Color,
    pub header: Color,
    pub border: Color,
    pub label: Color,
    pub label_dim: Color,
}

/// A channel/track strip container.
#[derive(Clone, PartialEq)]
pub struct StripStyle {
    pub body: Color,
    pub footer: Color,
    pub footer_text: Color,
    pub border: Color,
    pub accent: Color,
    pub value_text: Color,
}

/// A level meter's zone colours + well.
#[derive(Clone, PartialEq)]
pub struct MeterStyle {
    pub well: Color,
    pub border: Color,
    pub safe: Color,
    pub warn: Color,
    pub danger: Color,
    pub peak: Color,
}

/// A fader's cap + accent line.
#[derive(Clone, PartialEq)]
pub struct FaderStyle {
    pub well: Color,
    pub border: Color,
    pub cap_top: Color,
    pub cap_bottom: Color,
    pub accent: Color,
}

/// A toggle button (mute/solo/arm/…): off vs on appearance.
#[derive(Clone, PartialEq)]
pub struct ToggleStyle {
    pub on_fill: Color,
    pub on_text: Color,
    pub off_bg: Color,
    pub off_text: Color,
    pub off_border: Color,
}

/// Which toggle a [`Theme::toggle`] call is styling.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToggleKind {
    Mute,
    Solo,
    RecArm,
}

// ─────────────────────────────────────────────────────────────────────────────
// Theme — owned, cloneable; resolves styles from tokens + state.
// ─────────────────────────────────────────────────────────────────────────────

/// The active theme: semantic [`Tokens`] + [`Metrics`] + per-context WALTER
/// vocabularies (currently [`McpTheme`]). Owned & cloneable so it can be
/// runtime-switched, hot-reloaded, and (later) imported from a REAPER theme.
/// Presets are constructors (see [`Theme::dark`]).
#[derive(Clone, PartialEq, Debug)]
pub struct Theme {
    pub tokens: Tokens,
    pub metrics: Metrics,
    /// The MCP strip context — named layouts, per-element colour overrides,
    /// and `define_parameter`-style knobs (`super::mcp`).
    pub mcp: McpTheme,
    /// The TCP row context — the same element vocabulary laid out as a
    /// track-control row (REAPER's `tcp.*`).
    pub tcp: McpTheme,
    /// Runtime WALTER engine (imported REAPER themes): re-evaluates a
    /// context's layout at the renderer's *actual* px size, the way REAPER
    /// re-runs WALTER on resize. `None` = anchor-resolved baked layouts only.
    pub engine: Option<super::mcp::LayoutEngine>,
    /// The arrange-view context — ruler/grid/lane/item colours (REAPER themes
    /// these purely through the palette; no WALTER).
    pub arrange: super::arrange::ArrangeTheme,
    /// Arrange image chrome (REAPER 7 fixed-lane buttons) — separate
    /// struct so `ArrangeTheme` stays `Copy`.
    pub arrange_skin: super::arrange::ArrangeSkin,
    /// The transport context — `trans.*` layout + transport_* images.
    pub trans: super::trans::TransTheme,
    /// The envelope-control-panel context — `envcp.*` layout + envcp_* images.
    pub envcp: super::envcp::EnvcpTheme,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// FTS default dark theme.
    pub fn dark() -> Self {
        let c = Color::hex;
        Self {
            tokens: Tokens {
                surface: c("#09090b").unwrap(),
                surface_raised: c("#18181b").unwrap(),
                surface_sunken: c("#0a0a0c").unwrap(),
                border: c("#27272a").unwrap(),
                text: c("#e4e4e7").unwrap(),
                text_dim: c("#a1a1aa").unwrap(),
                text_faint: c("#71717a").unwrap(),
                accent: c("#38bdf8").unwrap(),
                neutral_track: c("#a1a1aa").unwrap(),
                meter_safe: c("#22c55e").unwrap(),
                meter_warn: c("#f59e0b").unwrap(),
                meter_danger: c("#ef4444").unwrap(),
                solo: c("#eab308").unwrap(),
                mute: c("#ef4444").unwrap(),
                rec: c("#ef4444").unwrap(),
                route_parent: c("#22c55e").unwrap(),
                route_send: c("#eab308").unwrap(),
                route_recv: c("#38bdf8").unwrap(),
            },
            metrics: Metrics::default(),
            mcp: McpTheme::fts_default(),
            tcp: McpTheme::fts_default_tcp(),
            engine: None,
            arrange: super::arrange::ArrangeTheme::fts_default(),
            arrange_skin: super::arrange::ArrangeSkin::default(),
            trans: super::trans::TransTheme::fts_default(),
            envcp: super::envcp::EnvcpTheme::fts_default(),
        }
    }

    /// Resolve a track's accent colour (track colour, or the neutral token).
    pub fn track_accent(&self, st: &ThemeState) -> Color {
        st.track_color.unwrap_or(self.tokens.neutral_track)
    }

    /// A surface tinted toward the track colour by the theme's tint strength.
    pub fn track_tint(&self, base: Color, st: &ThemeState) -> Color {
        match st.track_color {
            Some(tc) => base.mix(tc, self.metrics.track_tint),
            None => base,
        }
    }

    /// The meter zone colour for a normalized level (0–1).
    pub fn meter_zone(&self, level: f32) -> Color {
        if level > 0.9 {
            self.tokens.meter_danger
        } else if level > 0.75 {
            self.tokens.meter_warn
        } else {
            self.tokens.meter_safe
        }
    }

    // ── typed style getters ──

    pub fn panel(&self) -> PanelStyle {
        let t = &self.tokens;
        PanelStyle {
            surface: t.surface,
            header: t.surface_raised,
            border: t.border,
            label: t.text_dim,
            label_dim: t.text_faint,
        }
    }

    pub fn strip(&self, st: &ThemeState) -> StripStyle {
        let t = &self.tokens;
        let accent = self.track_accent(st);
        StripStyle {
            body: self.track_tint(t.surface_raised, st),
            footer: accent,
            footer_text: accent.on(),
            border: accent.with_alpha(0x66),
            accent,
            value_text: t.text_dim,
        }
    }

    pub fn meter(&self) -> MeterStyle {
        let t = &self.tokens;
        MeterStyle {
            well: t.surface_raised,
            border: t.border,
            safe: t.meter_safe,
            warn: t.meter_warn,
            danger: t.meter_danger,
            peak: t.text.with_alpha(0xb3),
        }
    }

    pub fn fader(&self, st: &ThemeState) -> FaderStyle {
        let t = &self.tokens;
        FaderStyle {
            well: t.surface_raised,
            border: t.border,
            cap_top: Color::rgb(0x6b, 0x6b, 0x76),
            cap_bottom: Color::rgb(0x3a, 0x3a, 0x42),
            accent: self.track_accent(st),
        }
    }

    pub fn toggle(&self, kind: ToggleKind) -> ToggleStyle {
        let t = &self.tokens;
        let on = match kind {
            ToggleKind::Mute => t.mute,
            ToggleKind::Solo => t.solo,
            ToggleKind::RecArm => t.rec,
        };
        ToggleStyle {
            on_fill: on,
            on_text: on.on(),
            off_bg: t.surface_sunken,
            off_text: on,
            off_border: on.with_alpha(0x55),
        }
    }
}

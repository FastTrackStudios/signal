//! Audio-plugin theme system — multi-theme switching.
//!
//! Provides three switchable design languages:
//! - **Flat**: minimal depth, solid colors, no shadows
//! - **Modern** (default): warm hybrid aesthetic with subtle shadows
//! - **Skeuomorphic**: maximum depth, rich gradients, heavy shadows
//!
//! Components call [`use_theme`] to read the current [`Theme`] from context.
//! The [`ThemeProvider`] / [`use_init_theme`] entry points wire up the signal.
//!
//! Static constants ([`ACCENT`], [`TEXT`], etc.) remain for widgets that need
//! a zero-context fallback; they mirror the [`Theme::modern`] defaults.

use dioxus::prelude::*;

// ═══════════════════════════════════════════════════════════════════
//  STATIC FALLBACK CONSTANTS
// ═══════════════════════════════════════════════════════════════════
// Used by widgets that don't read from context. Match `Theme::modern`.

pub const ACCENT: &str = "#c8a86e";
pub const TEXT: &str = "#d4d4d8";
pub const TEXT_DIM: &str = "#737380";
pub const BORDER: &str = "#2a2a30";
pub const SURFACE: &str = "#0c0c0f";
pub const TRACK: &str = "#252528";
pub const SIGNAL_MOD: &str = "#8b5cf6";
pub const SIGNAL_SAFE: &str = "#4ade80";
pub const SIGNAL_WARN: &str = "#f0c040";
pub const SIGNAL_DANGER: &str = "#ef5350";

// ═══════════════════════════════════════════════════════════════════
//  THEME VARIANT
// ═══════════════════════════════════════════════════════════════════

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ThemeVariant {
    Flat,
    #[default]
    Modern,
    Skeuomorphic,
}

impl ThemeVariant {
    pub fn label(self) -> &'static str {
        match self {
            Self::Flat => "FLAT",
            Self::Modern => "MODERN",
            Self::Skeuomorphic => "SKEUO",
        }
    }

    pub const ALL: [ThemeVariant; 3] = [Self::Flat, Self::Modern, Self::Skeuomorphic];
}

// ═══════════════════════════════════════════════════════════════════
//  THEME STRUCT
// ═══════════════════════════════════════════════════════════════════

/// Complete theme definition — all `&'static str` for zero-cost copies.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub variant: ThemeVariant,

    // ── Colors ────────────────────────────────────────────────────
    pub bg: &'static str,
    pub card_bg: &'static str,
    pub surface: &'static str,
    pub surface_raised: &'static str,
    pub surface_hover: &'static str,

    pub text: &'static str,
    pub text_dim: &'static str,
    pub text_bright: &'static str,

    pub accent: &'static str,
    pub accent_hover: &'static str,
    pub accent_dim: &'static str,
    pub accent_glow: &'static str,

    pub signal_safe: &'static str,
    pub signal_warn: &'static str,
    pub signal_danger: &'static str,
    pub signal_mod: &'static str,
    pub signal_safe_glow: &'static str,
    pub signal_warn_glow: &'static str,
    pub signal_danger_glow: &'static str,

    pub toggle_off: &'static str,
    pub border: &'static str,
    pub border_subtle: &'static str,
    pub grid_line: &'static str,
    pub crosshair: &'static str,
    pub reference_dot: &'static str,
    pub knob_track: &'static str,
    pub knob_body_light: &'static str,
    pub knob_body_dark: &'static str,
    pub knob_indicator: &'static str,

    // ── Shadows & Depth ──────────────────────────────────────────
    pub shadow_raised: &'static str,
    pub shadow_inset: &'static str,
    pub shadow_knob: &'static str,
    pub shadow_subtle: &'static str,
    pub shadow_glow: &'static str,
    pub highlight_top: &'static str,
    pub shadow_panel: &'static str,

    // ── Typography ──────────────────────────────────────────────
    pub font_family: &'static str,
    pub font_mono: &'static str,
    pub font_size_title: &'static str,
    pub font_size_value: &'static str,
    pub font_size_label: &'static str,
    pub font_size_tiny: &'static str,
    pub font_size_readout: &'static str,
    pub letter_spacing_label: &'static str,

    // ── Spacing ─────────────────────────────────────────────────
    pub spacing_root: &'static str,
    pub spacing_section: &'static str,
    pub spacing_control: &'static str,
    pub spacing_card: &'static str,
    pub spacing_label: &'static str,
    pub spacing_tight: &'static str,

    // ── Border Radius ───────────────────────────────────────────
    pub radius_card: &'static str,
    pub radius_button: &'static str,
    pub radius_small: &'static str,
    pub radius_round: &'static str,

    // ── Transitions ─────────────────────────────────────────────
    pub transition_fast: &'static str,
    pub transition_normal: &'static str,
}

const SHARED_TEXT: &str = "#d4d4d8";
const SHARED_TEXT_DIM: &str = "#737380";
const SHARED_TEXT_BRIGHT: &str = "#eeeeef";
const SHARED_FONT_FAMILY: &str = "system-ui, -apple-system, 'Segoe UI', sans-serif";
const SHARED_FONT_MONO: &str =
    "'SF Mono', 'Cascadia Code', 'JetBrains Mono', ui-monospace, monospace";

impl Theme {
    pub const fn flat() -> Self {
        Self {
            variant: ThemeVariant::Flat,
            bg: "#111114",
            card_bg: "#18181c",
            surface: "#0e0e12",
            surface_raised: "#202026",
            surface_hover: "#26262c",
            text: SHARED_TEXT,
            text_dim: SHARED_TEXT_DIM,
            text_bright: SHARED_TEXT_BRIGHT,
            accent: "#8fa8c8",
            accent_hover: "#a4bdd8",
            accent_dim: "rgba(143,168,200,0.20)",
            accent_glow: "transparent",
            signal_safe: "#4ade80",
            signal_warn: "#f0c040",
            signal_danger: "#ef5350",
            signal_mod: "#8b5cf6",
            signal_safe_glow: "rgba(74,222,128,0.3)",
            signal_warn_glow: "rgba(240,192,64,0.3)",
            signal_danger_glow: "rgba(239,83,80,0.4)",
            toggle_off: "#333338",
            border: "#2c2c34",
            border_subtle: "#242428",
            grid_line: "rgba(255,255,255,0.05)",
            crosshair: "rgba(255,100,100,0.3)",
            reference_dot: "rgba(255,255,255,0.10)",
            knob_track: "#252528",
            knob_body_light: "#333338",
            knob_body_dark: "#1c1c20",
            knob_indicator: "#d4d4d8",
            shadow_raised: "none",
            shadow_inset: "inset 0 1px 2px rgba(0,0,0,0.3)",
            shadow_knob: "0 1px 2px rgba(0,0,0,0.3)",
            shadow_subtle: "none",
            shadow_glow: "none",
            highlight_top: "none",
            shadow_panel: "none",
            font_family: SHARED_FONT_FAMILY,
            font_mono: SHARED_FONT_MONO,
            font_size_title: "16px",
            font_size_value: "12px",
            font_size_label: "10px",
            font_size_tiny: "9px",
            font_size_readout: "20px",
            letter_spacing_label: "0.6px",
            spacing_root: "10px 14px",
            spacing_section: "10px",
            spacing_control: "14px",
            spacing_card: "10px 12px",
            spacing_label: "4px",
            spacing_tight: "2px",
            radius_card: "6px",
            radius_button: "4px",
            radius_small: "3px",
            radius_round: "50%",
            transition_fast: "all 0.12s ease",
            transition_normal: "all 0.2s ease",
        }
    }

    pub const fn modern() -> Self {
        Self {
            variant: ThemeVariant::Modern,
            bg: "#111114",
            card_bg: "#1a1a1e",
            surface: "#0c0c0f",
            surface_raised: "#222228",
            surface_hover: "#28282e",
            text: SHARED_TEXT,
            text_dim: SHARED_TEXT_DIM,
            text_bright: SHARED_TEXT_BRIGHT,
            accent: "#c8a86e",
            accent_hover: "#dfc088",
            accent_dim: "rgba(200,168,110,0.25)",
            accent_glow: "rgba(200,168,110,0.35)",
            signal_safe: "#4ade80",
            signal_warn: "#f0c040",
            signal_danger: "#ef5350",
            signal_mod: "#8b5cf6",
            signal_safe_glow: "rgba(74,222,128,0.3)",
            signal_warn_glow: "rgba(240,192,64,0.3)",
            signal_danger_glow: "rgba(239,83,80,0.4)",
            toggle_off: "#333338",
            border: "#2a2a30",
            border_subtle: "#222228",
            grid_line: "rgba(255,255,255,0.05)",
            crosshair: "rgba(255,100,100,0.3)",
            reference_dot: "rgba(255,255,255,0.10)",
            knob_track: "#252528",
            knob_body_light: "#3a3a42",
            knob_body_dark: "#1a1a1e",
            knob_indicator: "#d4d4d8",
            shadow_raised: "0 2px 8px rgba(0,0,0,0.4), 0 1px 2px rgba(0,0,0,0.3)",
            shadow_inset: "inset 0 1px 3px rgba(0,0,0,0.5), inset 0 0 1px rgba(0,0,0,0.3)",
            shadow_knob: "0 2px 6px rgba(0,0,0,0.5), 0 1px 2px rgba(0,0,0,0.3)",
            shadow_subtle: "0 1px 3px rgba(0,0,0,0.3)",
            shadow_glow: "0 0 8px rgba(200,168,110,0.3)",
            highlight_top: "inset 0 1px 0 rgba(255,255,255,0.06)",
            shadow_panel: "0 2px 8px rgba(0,0,0,0.4), 0 1px 2px rgba(0,0,0,0.3), inset 0 1px 0 rgba(255,255,255,0.06)",
            font_family: SHARED_FONT_FAMILY,
            font_mono: SHARED_FONT_MONO,
            font_size_title: "16px",
            font_size_value: "12px",
            font_size_label: "10px",
            font_size_tiny: "9px",
            font_size_readout: "20px",
            letter_spacing_label: "0.6px",
            spacing_root: "10px 14px",
            spacing_section: "10px",
            spacing_control: "14px",
            spacing_card: "10px 12px",
            spacing_label: "4px",
            spacing_tight: "2px",
            radius_card: "6px",
            radius_button: "4px",
            radius_small: "3px",
            radius_round: "50%",
            transition_fast: "all 0.12s ease",
            transition_normal: "all 0.2s ease",
        }
    }

    pub const fn skeuomorphic() -> Self {
        Self {
            variant: ThemeVariant::Skeuomorphic,
            bg: "#0e0e12",
            card_bg: "#1c1c22",
            surface: "#0a0a0e",
            surface_raised: "#262630",
            surface_hover: "#2c2c36",
            text: SHARED_TEXT,
            text_dim: SHARED_TEXT_DIM,
            text_bright: SHARED_TEXT_BRIGHT,
            accent: "#d4a84a",
            accent_hover: "#e8c06a",
            accent_dim: "rgba(212,168,74,0.30)",
            accent_glow: "rgba(212,168,74,0.45)",
            signal_safe: "#4ade80",
            signal_warn: "#f0c040",
            signal_danger: "#ef5350",
            signal_mod: "#8b5cf6",
            signal_safe_glow: "rgba(74,222,128,0.3)",
            signal_warn_glow: "rgba(240,192,64,0.3)",
            signal_danger_glow: "rgba(239,83,80,0.4)",
            toggle_off: "#2e2e36",
            border: "#363640",
            border_subtle: "#2a2a34",
            grid_line: "rgba(255,255,255,0.06)",
            crosshair: "rgba(255,100,100,0.35)",
            reference_dot: "rgba(255,255,255,0.12)",
            knob_track: "#28282e",
            knob_body_light: "#454550",
            knob_body_dark: "#161618",
            knob_indicator: "#e0e0e4",
            shadow_raised: "0 3px 12px rgba(0,0,0,0.6), 0 1px 4px rgba(0,0,0,0.4)",
            shadow_inset: "inset 0 2px 5px rgba(0,0,0,0.6), inset 0 0 2px rgba(0,0,0,0.4)",
            shadow_knob: "0 3px 10px rgba(0,0,0,0.6), 0 1px 3px rgba(0,0,0,0.4), inset 0 1px 1px rgba(255,255,255,0.08)",
            shadow_subtle: "0 2px 5px rgba(0,0,0,0.4), 0 1px 2px rgba(0,0,0,0.3)",
            shadow_glow: "0 0 12px rgba(212,168,74,0.4)",
            highlight_top: "inset 0 1px 0 rgba(255,255,255,0.10)",
            shadow_panel: "0 3px 12px rgba(0,0,0,0.6), 0 1px 4px rgba(0,0,0,0.4), inset 0 1px 0 rgba(255,255,255,0.10)",
            font_family: SHARED_FONT_FAMILY,
            font_mono: SHARED_FONT_MONO,
            font_size_title: "16px",
            font_size_value: "12px",
            font_size_label: "10px",
            font_size_tiny: "9px",
            font_size_readout: "20px",
            letter_spacing_label: "0.6px",
            spacing_root: "10px 14px",
            spacing_section: "10px",
            spacing_control: "14px",
            spacing_card: "10px 12px",
            spacing_label: "4px",
            spacing_tight: "2px",
            radius_card: "6px",
            radius_button: "4px",
            radius_small: "3px",
            radius_round: "50%",
            transition_fast: "all 0.12s ease",
            transition_normal: "all 0.2s ease",
        }
    }

    pub const fn for_variant(variant: ThemeVariant) -> Self {
        match variant {
            ThemeVariant::Flat => Self::flat(),
            ThemeVariant::Modern => Self::modern(),
            ThemeVariant::Skeuomorphic => Self::skeuomorphic(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
//  COMPOSITE STYLE METHODS
// ═══════════════════════════════════════════════════════════════════

impl Theme {
    pub fn style_card(&self) -> String {
        format!(
            "background: {}; border: 1px solid {}; border-radius: {}; box-shadow: {};",
            self.card_bg, self.border, self.radius_card, self.shadow_panel,
        )
    }

    pub fn style_inset(&self) -> String {
        format!(
            "background: {}; border: 1px solid {}; border-radius: {}; box-shadow: {};",
            self.surface, self.border_subtle, self.radius_button, self.shadow_inset,
        )
    }

    pub fn style_label(&self) -> String {
        format!(
            "font-size: {}; font-weight: 600; color: {}; text-transform: uppercase; letter-spacing: {};",
            self.font_size_label, self.text_dim, self.letter_spacing_label,
        )
    }

    pub fn style_value(&self) -> String {
        format!(
            "font-family: {}; font-size: {}; font-variant-numeric: tabular-nums; \
             min-width:36px; text-align:center; color: {};",
            self.font_mono, self.font_size_value, self.text,
        )
    }

    pub fn style_readout(&self) -> String {
        format!(
            "font-family: {}; font-size: {}; font-variant-numeric: tabular-nums; font-weight: 300; color: {};",
            self.font_mono, self.font_size_readout, self.text,
        )
    }

    pub fn base_css(&self) -> String {
        format!(
            "*, *::before, *::after {{ box-sizing: border-box; margin: 0; padding: 0; }} \
             html, body {{ width: 100%; height: 100%; overflow: hidden; \
             background: {bg}; color: {text}; \
             font-family: {font}; font-size: 13px; }}",
            bg = self.bg,
            text = self.text,
            font = self.font_family,
        )
    }

    pub fn root_style(&self) -> String {
        format!(
            "width:100vw; height:100vh; padding:{pad}; \
             background:{bg}; color:{text}; \
             font-family:{font}; font-size:13px; user-select:none; \
             overflow-y:auto; position:relative;",
            pad = self.spacing_root,
            bg = self.bg,
            text = self.text,
            font = self.font_family,
        )
    }
}

// ═══════════════════════════════════════════════════════════════════
//  PROVIDER & HOOKS
// ═══════════════════════════════════════════════════════════════════

/// Wraps children and provides a `Signal<Theme>` via Dioxus context.
#[component]
pub fn ThemeProvider(children: Element) -> Element {
    let theme = use_signal(Theme::modern);
    use_context_provider(|| theme);
    rsx! { {children} }
}

/// Initialize the theme signal and provide it via context. Call once at the
/// root of your editor. Returns the signal so the same component can read it.
pub fn use_init_theme() -> Signal<Theme> {
    let theme = use_signal(Theme::modern);
    use_context_provider(|| theme);
    theme
}

/// Read the current theme from context. Requires an ancestor that called
/// [`use_init_theme`] or rendered a [`ThemeProvider`].
pub fn use_theme() -> Signal<Theme> {
    use_context::<Signal<Theme>>()
}

//! The arrangement's colours, as CSS.
//!
//! What is left of the component lanes: the arrangement is painted by
//! `session_daw::widget::ArrangementWidget`, and the DOM around it (the
//! panel's ground, its scrollbars, the toolbar) takes its colours from
//! here so the two agree.

/// The colours the lanes draw in, as CSS.
///
/// Resolved once from the theme so no render parses a token, and held as
/// strings because that is what a style attribute takes — the conversion
/// would otherwise happen per item per frame.
#[derive(Clone, PartialEq)]
pub struct Colors {
    pub surface: String,
    pub row_a: String,
    pub row_b: String,
    pub divider: String,
    pub grid: String,
    pub grid_beat: String,
    /// What a track with no colour of its own lends its items.
    pub uncoloured: String,
    /// The shade a fade lays over the part of an item it takes away.
    pub fade: String,
    pub text: String,
    /// The ruler's ground, and the ink its numbers are written in.
    pub ruler_bg: String,
    pub ruler_fg: String,
    /// The hairline under each of the ruler's rows.
    pub rule: String,
    /// What a row's own name is written in, beside its contents.
    pub faint: String,
    /// The accent, which a tempo change is marked with — and which a
    /// lit rail button takes as its face.
    pub accent: String,
    /// Ink that reads on the accent, which is black or near-white
    /// depending on how light the accent is. Resolved once here rather
    /// than guessed per control, because a theme with a pale accent and
    /// a theme with a deep one want opposite answers.
    pub ink_on_accent: String,
    /// The rails' own ground, which is the window's gutter.
    pub tcp_gutter: String,
    /// An unlit control's face.
    pub button: String,
    /// The panel's row tint before a track's colour is mixed into it,
    /// and the same as channels so a mix does not have to parse it back.
    pub tcp_tint: String,
    pub tcp_tint_rgb: (u8, u8, u8),
    /// The panel's left column, its field, and how strongly a track's
    /// colour tints its row.
    pub tcp_column: String,
    pub tcp_field: String,
    /// A combo box's well, which is sunk further than a field.
    pub tcp_combo: String,
    pub track_tint: f32,
    /// And the ink on it.
    pub text_dim: String,
}

impl Colors {
    /// The arrangement's own colours, off the theme.
    #[must_use]
    pub fn from_theme(theme: &crate::theming::Theme) -> Self {
        let c = |col: crate::theming::Color| rgba(col.r, col.g, col.b, f64::from(col.a) / 255.0);
        Self {
            surface: c(theme.arrange.bg),
            row_a: c(theme.arrange.row_bg[0]),
            row_b: c(theme.arrange.row_bg[1]),
            divider: c(theme.arrange.row_divider[0]),
            grid: c(theme.arrange.grid_measure),
            grid_beat: c(theme.arrange.grid_beat),
            uncoloured: c(theme.tokens.text_faint),
            ruler_bg: c(theme.arrange.ruler_bg),
            ruler_fg: c(theme.arrange.ruler_fg),
            rule: c(theme.tokens.border),
            faint: c(theme.tokens.text_faint),
            accent: c(theme.tokens.accent),
            ink_on_accent: ink_on(theme.tokens.accent),
            tcp_gutter: c(theme.tokens.surface),
            tcp_tint: c(theme.tokens.surface_raised),
            tcp_tint_rgb: (
                theme.tokens.surface_raised.r,
                theme.tokens.surface_raised.g,
                theme.tokens.surface_raised.b,
            ),
            tcp_column: c(theme.tokens.surface_sunken),
            tcp_field: c(theme.tokens.surface_sunken),
            tcp_combo: c(theme.tokens.surface_sunken),
            track_tint: theme.metrics.track_tint,
            button: c(theme.tokens.surface),
            text_dim: c(theme.tokens.text_dim),
            // The recorded scene's own fade shade, as CSS.
            fade: rgba(0, 0, 0, 0.45),
            text: c(theme.tokens.text),
        }
    }
}

impl Default for Colors {
    fn default() -> Self {
        Self::from_theme(&crate::theming::Theme::default())
    }
}

/// Ink that reads on a background.
///
/// Black on anything light, near-white on anything dark, by relative
/// luminance. The floor is the one the painted window uses, so a lit
/// control letters the same way in both.
#[must_use]
pub fn ink_on(background: crate::theming::Color) -> String {
    /// Above this the background is light enough for black ink.
    ///
    /// Low, and deliberately so: these are saturated mid-tones and black
    /// on them reads as a number stamped on a colour, where a light ink
    /// reads as a second label floating over it. The painted window uses
    /// the same floor, so a lit control letters the same way in both.
    const FLOOR: f32 = 0.179;
    let at = |v: u8| f32::from(v) / 255.0;
    let luminance = 0.2126_f32.mul_add(
        at(background.r),
        0.7152_f32.mul_add(at(background.g), 0.0722 * at(background.b)),
    );
    if luminance > FLOOR {
        rgba(0, 0, 0, 1.0)
    } else {
        rgba(0xe8, 0xe8, 0xea, 1.0)
    }
}

/// `rgba(r, g, b, a)`, which is what a style attribute wants.
fn rgba(r: u8, g: u8, b: u8, a: f64) -> String {
    format!("rgba({r}, {g}, {b}, {a:.3})")
}

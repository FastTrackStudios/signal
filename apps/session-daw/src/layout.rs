//! How tall rows are drawn.
//!
//! The height itself comes from the track — `daw_proto::Track::height`,
//! which the standalone loader fills in from the project's
//! `TRACKHEIGHT` and the REAPER backend from `I_HEIGHTOVERRIDE`. What is
//! here is everything the TRACK does not say: what a track with no
//! stored height gets, and how small a row is allowed to become.
//!
//! Both are settings rather than constants, because both are preferences
//! and people disagree about them. A default track height is the first
//! thing anyone changes about a DAW's track panel, and the floor is the
//! whole argument of this module — see [`Layout::min`].

/// Row heights, as the user wants them.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    /// What a track with no stored height is drawn at.
    ///
    /// `Track::height` is `None` for a track nobody has resized, which
    /// is most of them in most projects, so this is the height of
    /// essentially every row in a fresh session. It belongs to the user,
    /// not to the file and not to this crate.
    ///
    /// The default is [`CONTROL_ROW`] — the smallest height that holds
    /// the whole control row at its authored size. A resting session
    /// should be dense, but a track nobody has touched should still have
    /// its knobs where its knobs go; seeing the whole session is what
    /// the vertical ZOOM is for, not what the default height is for.
    pub default: f64,
    /// What a track with no stored strip width is drawn at.
    pub strip: f64,
    /// The narrowest a strip may be set to.
    pub strip_min: f64,
    /// The smallest a track may be SET to.
    ///
    /// Deliberately not the smallest a row can be DRAWN at: that is
    /// [`crate::tcp::BAND_BELOW`] and it applies to the height a row
    /// lands at on screen, after the zoom. A track sized down to nothing
    /// is a track whose controls you can no longer reach at any zoom,
    /// where a session zoomed out is one gesture away from being
    /// readable again — so the floor on the stored height is generous
    /// and the floor on the drawn height is a single pixel.
    pub min: f64,
}

/// The mixer's default strip width — REAPER's, which is the only one it
/// has.
pub const STRIP_WIDE: f64 = 86.0;

/// The narrowest a strip may be set to.
///
/// Enough for the name, mute and solo, and the fader's own body. A strip
/// below this is not a narrow strip, it is a strip with its controls cut
/// off — and unlike a short row, there is no second form for them to
/// take: a fader is a fader at any width.
pub const STRIP_NARROW: f64 = 30.0;

/// The smallest row holding the whole control row at its authored size.
///
/// Row one is 24 tall and the band is the row less a pixel top and
/// bottom, so this is that plus its margins. Below it the controls start
/// scaling down; at it, a track nobody has resized looks like a track.
pub const CONTROL_ROW: f64 = 32.0;

/// The smallest row that still prints a legible track name.
///
/// Not a taste: it is the name's own type size plus the space a line of
/// it needs. Below this a row stops carrying a name at all (see
/// [`crate::tcp::Density`]), so it is the boundary between "a list of
/// tracks" and "a picture of a session" — which the vertical zoom
/// crosses, and a stored height no longer does.
pub const NAME_LEGIBLE: f64 = 14.0;

impl Default for Layout {
    fn default() -> Self {
        Self {
            default: CONTROL_ROW,
            // The control row at its authored size, plus a pixel top
            // and bottom.
            //
            // Every control is ONE size on every track — see
            // `tcp::row_one` — so this is not "the smallest row that can
            // show something", it is the smallest row that can show the
            // controls as they are drawn everywhere else. A floor below
            // it would mean shrinking them per track, and a control that
            // is a different shape on every track cannot be built on.
            //
            // Seeing a whole session is the vertical ZOOM's job, and the
            // zoom still takes a row down to a single pixel.
            min: CONTROL_ROW - 6.0,
            strip: STRIP_WIDE,
            strip_min: STRIP_NARROW,
        }
    }
}

impl Layout {
    /// Read the overrides from the environment.
    ///
    /// A stand-in for the settings store this window does not have yet.
    /// It is here rather than at the call site so that when there IS a
    /// settings store, one function changes and no caller does.
    #[must_use]
    pub fn from_env() -> Self {
        let mut layout = Self::default();
        if let Some(height) = number("FTS_TRACK_HEIGHT") {
            layout.default = height;
        }
        if let Some(min) = number("FTS_TRACK_HEIGHT_MIN") {
            layout.min = min;
        }
        if let Some(width) = number("FTS_STRIP_WIDTH") {
            layout.strip = width;
        }
        if let Some(min) = number("FTS_STRIP_WIDTH_MIN") {
            layout.strip_min = min;
        }
        layout
    }

    /// How wide to draw a track's mixer strip.
    ///
    /// REAPER has no strip width — every strip is one width — so this is
    /// entirely ours, and `None` means the user's default rather than
    /// the host's.
    #[must_use]
    pub fn width_of(self, stored: Option<u32>) -> f64 {
        stored
            .map(f64::from)
            .filter(|w| *w > 0.0)
            .unwrap_or(self.strip)
            .max(self.strip_min)
    }

    /// How tall a track's slot is: the track, and its automation lanes
    /// under it ([`crate::automation`]).
    #[must_use]
    pub fn slot_of(self, track: &daw_proto::Track) -> f64 {
        self.height_of(track.height) + crate::automation::lanes_height(&track.guid)
    }

    /// How tall to draw a track.
    ///
    /// Anything at or below zero is treated as unset: a stored height of
    /// nought is not a row, and a negative one is a corrupt file rather
    /// than an instruction.
    #[must_use]
    pub fn height_of(self, stored: Option<u32>) -> f64 {
        stored
            .map(f64::from)
            .filter(|h| *h > 0.0)
            .unwrap_or(self.default)
            .max(self.min)
    }
}

/// How a strip carries its track's colour where there is no control
/// to carry it: the rack below the chain, and the rule up the left
/// edge. Each is one of three, from the environment.
///
/// - `FTS_RACK_FILL`: `off` (the panel's grey), `tint` or `track`
///   (the band's muted tint — the default), `full` (the track's
///   colour, the same as a full edge rule).
/// - `FTS_STRIP_EDGE`: `full` (the track's colour), `tint` (the band's
///   muted tint), `off` (the default — the gap between strips is the
///   divider).
/// - `FTS_STRIP_FILL`: the strip's own ground under the controls, FX
///   section included — `off` (the panel's grey — the default),
///   `tint`, `full`.
///
/// Settings rather than decisions: the grey says "room for more", the
/// colour says "this is the track's", and which one a mixer wants is
/// taste.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wash {
    Off,
    Tint,
    Full,
}

impl Wash {
    fn parse(value: &str, default: Self) -> Self {
        match value.trim().to_lowercase().as_str() {
            "off" | "0" | "none" | "false" => Self::Off,
            "tint" | "track" | "1" | "on" | "true" => Self::Tint,
            "full" | "color" | "colour" => Self::Full,
            _ => default,
        }
    }

    fn from_env(key: &str, default: Self) -> Self {
        std::env::var(key).map_or(default, |v| Self::parse(&v, default))
    }
}

/// What the rack paints under its chain — see [`Wash`].
#[must_use]
pub fn rack_fill() -> Wash {
    static ON: std::sync::OnceLock<Wash> = std::sync::OnceLock::new();
    *ON.get_or_init(|| Wash::from_env("FTS_RACK_FILL", Wash::Tint))
}

/// What the strip's own ground — under the FX section, the band, the
/// fader and the buttons — is painted in. See [`Wash`].
#[must_use]
pub fn strip_fill() -> Wash {
    static ON: std::sync::OnceLock<Wash> = std::sync::OnceLock::new();
    *ON.get_or_init(|| Wash::from_env("FTS_STRIP_FILL", Wash::Off))
}

/// What the rule up a strip's left edge is drawn in — see [`Wash`].
#[must_use]
pub fn strip_edge() -> Wash {
    static ON: std::sync::OnceLock<Wash> = std::sync::OnceLock::new();
    *ON.get_or_init(|| Wash::from_env("FTS_STRIP_EDGE", Wash::Off))
}

/// How much the racks of the strips that are NOT selected are
/// darkened, 0..1. The rack only — the strip's own controls stay as
/// they are.
///
/// `FTS_DIM_UNSELECTED`: `off` (or `0`) for none, a number for that
/// much, unset for the default. On by default: the selected strip is
/// the one you are working on, and a mixer where every strip is as
/// bright as that one is a mixer you have to find it in. Not so much
/// that the rest stops being readable — it is still the mixer.
#[must_use]
pub fn dim_unselected() -> f32 {
    static ON: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let Ok(value) = std::env::var("FTS_DIM_UNSELECTED") else {
            return DIM_DEFAULT;
        };
        let value = value.trim().to_lowercase();
        if matches!(value.as_str(), "off" | "none" | "false") {
            return 0.0;
        }
        value
            .parse::<f32>()
            .map_or(DIM_DEFAULT, |n| n.clamp(0.0, 0.9))
    })
}

/// The default darkening of an unselected strip.
const DIM_DEFAULT: f32 = 0.3;

/// A positive number from the environment, if it is one.
fn number(key: &str) -> Option<f64> {
    std::env::var(key)
        .ok()?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|n| *n > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unset_height_takes_the_default() {
        let layout = Layout {
            default: 48.0,
            ..Layout::default()
        };
        assert!((layout.height_of(None) - 48.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_stored_height_wins() {
        let layout = Layout::default();
        // Above the floor, so the stored value is what comes back
        // rather than the clamp — `the_floor_holds` covers the other side.
        assert!((layout.height_of(Some(96)) - 96.0).abs() < f64::EPSILON);
    }

    /// Zero is REAPER's sentinel for automatic, not a row with no height.
    #[test]
    fn zero_is_unset_not_flat() {
        let layout = Layout {
            default: 70.0,
            ..Layout::default()
        };
        assert!((layout.height_of(Some(0)) - 70.0).abs() < f64::EPSILON);
    }

    /// The floor applies to a stored height too — a project can ask for
    /// something smaller than anything can be drawn at.
    #[test]
    fn the_floor_holds() {
        let layout = Layout {
            default: 70.0,
            min: 6.0,
            ..Layout::default()
        };
        assert!((layout.height_of(Some(1)) - 6.0).abs() < f64::EPSILON);
    }

    /// A strip with no stored width takes the default, and one set
    /// narrower than the floor is held at it.
    #[test]
    fn strip_widths_default_and_clamp() {
        let layout = Layout::default();
        assert!((layout.width_of(None) - STRIP_WIDE).abs() < f64::EPSILON);
        assert!((layout.width_of(Some(120)) - 120.0).abs() < f64::EPSILON);
        assert!((layout.width_of(Some(4)) - STRIP_NARROW).abs() < f64::EPSILON);
    }

    /// The stored floor is generous; the DRAWN floor is not. A track
    /// cannot be sized into invisibility, but zooming out may draw it
    /// as a band — those are different limits and must not be confused.
    #[test]
    fn the_stored_floor_is_not_the_drawn_floor() {
        assert!(Layout::default().min > crate::tcp::BAND_BELOW);
    }
}

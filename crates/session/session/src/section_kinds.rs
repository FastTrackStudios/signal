//! Section / marker vocabulary, shared by the REAPER actions and the
//! browser setlist engine.
//!
//! These three enums used to live in [`crate::keyflow::actions`], which
//! is `#[cfg(not(target_arch = "wasm32"))]` because the rest of that
//! module drives REAPER through `daw::service`. But they are plain
//! data, and `setlist::chart_import` + `setlist::service::demo` — both
//! compiled for wasm, both used by the browser player in
//! `task-player-ui` — import them.
//!
//! The result was that `session` did not compile for wasm at all, which
//! failed the `task-web` image build and so the whole deploy. It was
//! invisible locally because a plain `cargo check` builds for the host,
//! and invisible to a `dx build` of the app because that had already
//! been done before the imports landed.
//!
//! So the vocabulary lives here, unconditionally, and
//! `keyflow::actions` re-exports it for the native call sites.

/// A Keyflow hotkey action. The REAPER-side dispatch for these lives in
/// `keyflow::actions`; the type is here so wasm callers can name one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyflowAction {
    InsertSection(SectionKind),
    InsertMarker(MarkerKind),
    ConvertMarkersToSessionFormat,
}

/// Song-section vocabulary — what an arrangement is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    Intro,
    Verse,
    PreChorus,
    Chorus,
    Bridge,
    Outro,
    Instrumental,
    Solo,
    Hits,
    Interlude,
    Breakdown,
    Vamp,
    Refrain,
    Tag,
    Turnaround,
    CountIn,
    End,
}

/// Point markers, as distinct from the ranged [`SectionKind`] regions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkerKind {
    CountIn,
    Start,
    End,
    SongStart,
    SongEnd,
}

impl MarkerKind {
    /// The plain RGB each marker gets — the single source both
    /// `keyflow::actions::MarkerKind::default_color` (packed into
    /// REAPER's native marker-color format) and a toolbar CSS pill
    /// (`css_color`) build on, so the REAPER-side marker and the
    /// standalone toolbar button render the literal same color.
    #[must_use]
    pub const fn rgb(self) -> u32 {
        match self {
            Self::CountIn => 0x00EC_4899,   // pink-500
            Self::SongStart => 0x0022_C55E, // green-500
            Self::Start => 0x0073_7373,     // neutral-500
            Self::End => 0x00DC_2626,       // red-600
            Self::SongEnd => 0x0047_5569,   // slate-600
        }
    }

    /// `rgb()` as a CSS hex string (`#RRGGBB`).
    #[must_use]
    pub fn css_color(self) -> String {
        format!("#{:06X}", self.rgb())
    }
}

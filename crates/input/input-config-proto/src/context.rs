//! Portable binding-context enums, extracted from reaper-input's
//! `input::keybinds` so wasm clients can use the config types without
//! linking the REAPER runtime. reaper-input re-exports these at their
//! original paths.

use facet::Facet;

/// Keybind context — where a keyboard/wheel binding applies.
///
/// The DAW-specific variants (`Main`/`Midi`/`MidiInline`/`MediaExplorer`)
/// are REAPER's editor sections. `Custom` keeps the enum **open** so
/// app-agnostic consumers (e.g. a general Dioxus app) can name their own
/// contexts — its payload is the raw context tag used in when-expressions
/// (`context:<name>`). Because of `Custom`, this enum is no longer `Copy`;
/// clone it where a value is needed from behind a reference.
#[derive(Debug, Clone, Facet, PartialEq, Eq, Hash, Default)]
#[repr(C)]
pub enum KeybindContext {
    /// All contexts (default)
    #[default]
    Global,
    /// Arrange view (main window)
    Main,
    /// MIDI editor window
    Midi,
    /// Inline MIDI editor
    MidiInline,
    /// Media explorer
    MediaExplorer,
    /// App-defined context, keyed by an arbitrary tag name. Round-trips
    /// through Facet/styx like any other variant.
    Custom(String),
}

impl KeybindContext {
    /// Check if this context matches another (Global matches everything)
    pub fn matches(&self, other: &KeybindContext) -> bool {
        *self == KeybindContext::Global || *other == KeybindContext::Global || *self == *other
    }
}

/// Mouse modifier context - where mouse modifiers apply
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq, Hash, Default)]
#[repr(C)]
pub enum MouseModifierContext {
    /// Media item left edge
    #[default]
    MediaItemLeftEdge,
    /// Media item right edge
    MediaItemRightEdge,
    /// Media item bottom half
    MediaItemBottomHalf,
    /// Media item fade/autocrossfade
    MediaItemFade,
    /// Envelope point
    EnvelopePoint,
    /// Envelope segment
    EnvelopeSegment,
    /// Track control panel
    TrackControlPanel,
    /// Arrange view (empty area)
    ArrangeView,
    /// MIDI note
    MidiNote,
    /// MIDI CC lane
    MidiCCLane,
    /// The timeline ruler — where you set the edit cursor and drag a
    /// time selection. REAPER models this as its own set of contexts
    /// (ruler, region lane, marker lane, tempo lane) because what a
    /// click does there depends on which lane it lands in.
    Ruler,
    /// A mixer strip's body.
    MixerStrip,
    /// App-defined context, so a surface can name one this enum does
    /// not have without every consumer needing a new variant.
    ///
    /// The keybind side already works this way — `KeybindContext` has
    /// had `Custom` since it was extracted — and a closed mouse-context
    /// enum is what forces a fork every time a new surface appears.
    Custom(&'static str),
}

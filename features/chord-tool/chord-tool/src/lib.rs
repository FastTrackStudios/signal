//! chord-tool — the chord-firing panel.
//!
//! GUI only. This is the `ChordGun` workflow (pick a key, hear and place
//! scale chords) but none of its theory: `ChordGun` carries its own
//! scales/chords tables, and everything they describe already exists in
//! `keyflow`. Pitches come from `keyflow::chord::realize`; if logic in
//! here starts computing intervals, it belongs there instead.
//!
//! The same [`ChordToolPanel`] runs two ways — a desktop window for
//! iteration (`cargo run -p chord-tool --example panel`) and a Blitz-
//! rendered REAPER panel via `fts-extensions`.

pub mod panel;
pub mod sink;

pub use panel::ChordToolPanel;
pub use sink::{ChordSink, LogSink, SinkHandle};

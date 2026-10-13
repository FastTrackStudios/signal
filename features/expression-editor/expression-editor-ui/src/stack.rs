//! The stacked multitrack view — every track at once, on one timeline.
//!
//! Each track gets a horizontal lane and is drawn in *its own* mode: a
//! vocal as blobs, its reference MIDI as a roll, a guitar as tab, a kit
//! as slices. Time is shared; vertical space is divided.
//!
//! That sharing is the entire feature, so it has to be exact. Two things
//! make it non-trivial:
//!
//! **Tracks do not agree on a time unit.** A pitched-audio document is
//! in analysis frames at the pitch tracker's hop; a percussive one is in
//! frames at the onset hop; a MIDI one is in ticks. Drawing all three
//! against the active track's camera without converting puts them at
//! wildly different scales — and it *looks* plausible, which is worse
//! than looking broken. Everything is converted through seconds.
//!
//! **Row spaces have different extents.** 128 pitch rows, 6 strings, 3
//! bands. Giving each lane the same rows-per-pixel would leave the kit
//! occupying a twentieth of its lane. Each lane instead fits its own
//! content to its own height.

// The geometry, the painter, the waveform summing and the drag-zoom
// live in `expression-editor-paint::stack`; the component and its
// element markup stay here.
use expression_editor_paint::stack::{geometry, paint, waveform, zoom};
/// The same picture as `paint`, as elements — for renderers that are
/// fast with them (WebView, web). See its module docs for why both.
#[cfg(feature = "webview")]
mod markup;
mod view;

pub use expression_editor_core::drum::HitGesture;
pub use geometry::{LaneNote, LaneView, SubLane, chrome_shelves, lanes, ruler_height};
pub use view::{StackView, StackViewProps};
pub use waveform::summed_columns;

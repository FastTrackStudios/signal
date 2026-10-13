//! The expression editor's tools, written once over every kind of event.
//!
//! One editor serves seven modes. The modes differ in what they draw and
//! what a gesture means; they do **not** differ in what Quantize does.
//! So a tool lives here, generic over [`Timed`], and each domain hands
//! it whatever it has:
//!
//! | Domain | Event | Has an end? |
//! |---|---|---|
//! | MIDI / MPE / Guitar | [`expression_editor_core::doc::Note`] | yes |
//! | Vocals (analysed) | `tune_dsp::model::NoteBlob` | yes |
//! | Drums / unpitched audio | `expression_editor_audio::detect::Transient` | no |
//!
//! [`Sustained`] is the difference in that last column, and it is the
//! only place the difference appears. See [`event`] for the shape of the
//! seam and why it is drawn there.
//!
//! ```
//! use expression_editor_core::doc::{Note, NoteId};
//! use expression_editor_tools::quantize::{self, QuantizeConfig};
//!
//! // Two 16ths on a 96-ppq grid, both a tick late.
//! let mut notes = vec![
//!     Note::new(NoteId(1), 25.0, 49.0, 60),
//!     Note::new(NoteId(2), 49.0, 73.0, 62),
//! ];
//! let plan = quantize::plan(&notes, QuantizeConfig { grid: 24.0, ..Default::default() });
//! quantize::apply(&mut notes, &plan);
//!
//! assert_eq!((notes[0].start, notes[0].end), (24.0, 48.0));
//! assert_eq!((notes[1].start, notes[1].end), (48.0, 72.0));
//! ```

pub mod align;
pub mod event;
// Absorbed from the `midi-tools` crates (#153). Velocity and arp are
// the same shape as the tools already here — an operation over selected
// events with a config — so they belong beside them rather than in
// three crates of their own.
pub mod arp;
pub mod note_shape;
pub mod quantize;
pub mod sink;
pub mod velocity;

pub use event::{Sustained, Timed, length_of};

pub use sink::{ArpSink, DemoArpSink, DemoSink, VelocitySink};
pub use velocity::{Note, Range, Session, VelocityEdit};

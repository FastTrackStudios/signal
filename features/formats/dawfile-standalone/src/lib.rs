//! The `.session` project format.
//!
//! FastTrackStudio's native project format, built to the design settled in
//! [#155][]. Styx on the inside under an aliased extension; the readable,
//! hand-editable **source of truth**; persisting `daw_proto` through `Facet`
//! rather than a third invented model; a project directory of exactly two
//! entries; REAPER `.rpp` as a proven-lossless round trip and DAWproject as
//! interop rather than a native format.
//!
//! The extension is `.session` — a session is what the thing *is*, and it
//! is what the app, the session domain and the user all call it. Projects
//! written by an earlier build as `.daw` still open; the styx inside is
//! byte-for-byte the same, only the name changed.
//!
//! [#155]: https://github.com/FastTrackStudios/FastTrackStudio/issues/155
//!
//! ## The shape of a project
//!
//! ```text
//! Belief.session/
//!   Belief.session      styx — tracks, items, takes, envelopes, tempo map,
//!                       markers, routing, editor state, provenance
//!   objects/            immutable, content-addressed blobs — FX chains,
//!                       MIDI source data, the verbatim imported source
//! ```
//!
//! Everything mutable is small and textual; everything large is immutable
//! and hash-named. That split is not an implementation detail — it is what
//! makes sync conflicts on large data structurally impossible, and the
//! implementation must not break it.
//!
//! ## The one rule that costs nothing now and everything later
//!
//! Every entity — track, item, take, envelope, marker — carries a stable
//! unique id, and **nothing is ever referenced by array index or position**.
//! See [`id`] for what that means in practice, including what deliberately
//! is *not* an entity and why the `index` fields on `daw_proto`'s types are
//! a derived cache rather than an address.
//!
//! ## Getting started
//!
//! ```no_run
//! use dawfile_standalone::{DawProject, DocumentEdit, DocumentQuery};
//!
//! // Import a REAPER project. The original bytes are kept verbatim.
//! let (mut project, report) = DawProject::import_rpp_file("Belief.RPP")?;
//! for (key, count) in report.ranked().iter().take(5) {
//!     println!("not modelled: {key} x{count}");
//! }
//!
//! // Untouched, it exports back byte-for-byte.
//! assert_eq!(project.to_rpp()?, std::fs::read_to_string("Belief.RPP")?);
//!
//! // Edit by id, never by position.
//! let track = project.document().tracks[0].id.clone();
//! project.edit(|document| {
//!     document.track_mut(&track).unwrap().track.volume = 0.5;
//! });
//!
//! // Save as a `.session` project directory.
//! project.save("Belief.session")?;
//! # Ok::<(), dawfile_standalone::DawError>(())
//! ```
//!
//! ## What lives elsewhere
//!
//! The object store's GC and `compact` ([#172][]), the persisted loro oplog
//! and offline merge ([#173][]), DAWproject import/export ([#174][]) and
//! templates ([#175][]) are separate tickets. Nothing here forecloses them —
//! see [`objects`] and [`project`] for the seams each attaches to.
//!
//! [#172]: https://github.com/FastTrackStudios/FastTrackStudio/issues/172
//! [#173]: https://github.com/FastTrackStudios/FastTrackStudio/issues/173
//! [#174]: https://github.com/FastTrackStudios/FastTrackStudio/issues/174
//! [#175]: https://github.com/FastTrackStudios/FastTrackStudio/issues/175

/// The CRDT the project history is kept in, for callers that hand one
/// in ([`project::DawProject::set_history`]).
pub use loro;

pub mod dawproject;
pub mod document;
pub mod edit;
pub mod error;
pub mod id;
pub mod objects;
pub mod oplog;
pub mod project;
pub mod rpp;
pub mod styx;
pub mod template;

pub use document::{
    DawDocument, EditorState, EnvelopeNode, FORMAT_VERSION, ItemNode, MarkerNode, Provenance,
    ReceiveNode, SourceFormat, SourceRef, TakeNode, TrackNode,
};
pub use edit::{DocumentEdit, DocumentQuery};
pub use error::{DawError, DawResult};
pub use id::{EntityId, ObjectId};
pub use objects::ObjectStore;
pub use project::{
    DAW_EXTENSION, DawProject, OBJECTS_DIR, PROJECT_EXTENSIONS, SESSION_EXTENSION, choose_manifest,
};
pub use rpp::{ExportReport, ImportReport};

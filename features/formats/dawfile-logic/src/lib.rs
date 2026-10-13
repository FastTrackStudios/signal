// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! Logic Pro session file parser (`.logicx`) — pure Rust, no C dependencies.
//!
//! Parses `.logicx` bundle directories produced by Logic Pro (versions 10.4+
//! through 12.x), extracting project metadata, tracks, markers, tempo maps,
//! and summing groups.
//!
//! # Bundle structure
//!
//! A `.logicx` file is a **directory bundle**:
//!
//! ```text
//! MyProject.logicx/
//! ├── Resources/
//! │   └── ProjectInformation.plist   # creator version, variant names
//! └── Alternatives/
//!     ├── 000/                       # earliest saved alternative
//!     │   ├── MetaData.plist
//!     │   └── ProjectData
//!     └── NNN/                       # latest alternative (selected automatically)
//!         ├── MetaData.plist         # BPM, time sig, sample rate, key, track count
//!         └── ProjectData            # binary chunk stream
//! ```
//!
//! # Binary format
//!
//! `ProjectData` is a proprietary binary chunk stream beginning with a 24-byte
//! file header (magic `#G` / `0x2347`) followed by 36-byte chunk headers and
//! variable-length payloads.  The format is partially reverse-engineered from
//! the [cigol](https://gitlab.com/fastfourier666/cigol) project.
//!
//! # Supported features
//!
//! | Feature | Status |
//! |---------|--------|
//! | Project metadata (BPM, time sig, key, sample rate) | ✓ |
//! | Track names and kinds (from mixer Envi chunks) | ✓ partial |
//! | Summing groups | ✓ partial |
//! | Chunk inventory | ✓ (always populated) |
//! | Arrangement markers | TODO |
//! | Tempo change events | TODO |
//! | Audio clips (name, duration, source offset) | ✓ partial |
//! | Automation | TODO |
//! | Write / export | TODO |
//!
//! # Example
//!
//! ```no_run
//! let session = dawfile_logic::read_session("MyProject.logicx")?;
//!
//! println!("{}", dawfile_logic::session_summary(&session));
//! for chunk in &session.chunks {
//!     println!("  {:?} @ {:#x} ({} bytes)", chunk.type_name, chunk.offset, chunk.data_len);
//! }
//! # Ok::<(), dawfile_logic::LogicError>(())
//! ```
//!
//! # Capability declaration
//!
//! ```rust
//! let support = dawfile_logic::feature_support();
//! assert!(support.can_read(daw_proto::Capability::Tracks));
//! ```

#![deny(unsafe_code)]

pub mod convert;
pub mod error;
pub mod io;
pub mod parse;
pub mod types;

// ── Public API ────────────────────────────────────────────────────────────────

pub use convert::{feature_support, session_summary};
pub use error::{LogicError, LogicResult};
pub use io::read_session;
pub use types::{
    ClipKind, LogicChunk, LogicClip, LogicCompRange, LogicMarker, LogicMidiNote, LogicSession,
    LogicSummingGroup, LogicTake, LogicTakeFolder, LogicTempoEvent, LogicTrack, TrackKind,
};

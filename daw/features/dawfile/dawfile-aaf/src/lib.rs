// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! AAF (Advanced Authoring Format) session file parser.
//!
//! Parses `.aaf` files produced by professional video/audio tools — Pro Tools,
//! Avid Media Composer, Adobe Premiere, DaVinci Resolve, Logic Pro, Fairlight,
//! and many others — extracting tracks, clips, markers, timecode, and audio
//! essence metadata.
//!
//! Implemented in **pure Rust** with no C library dependencies.  The container
//! layer uses the [`cfb`](https://crates.io/crates/cfb) crate
//! (MIT) for Compound File Binary parsing.  The AAF object model layer is
//! implemented from scratch guided by SMPTE ST 2001-1 and the
//! [pyaaf2](https://github.com/markreidvfx/pyaaf2) MIT-licensed Python
//! reference implementation.
//!
//! # Supported AAF features
//!
//! | Feature | Status |
//! |---------|--------|
//! | Audio tracks (TimelineMobSlots) | ✓ |
//! | Video tracks | ✓ |
//! | Source clip resolution (Comp → Master → Source) | ✓ |
//! | Filler / silence gaps | ✓ |
//! | Transitions (dissolves, fades) | ✓ |
//! | Operation groups (speed changes, gain ramps) | ✓ partial |
//! | Timeline markers (CommentMarkers) | ✓ |
//! | SMPTE timecode | ✓ |
//! | Audio essence metadata (PCMDescriptor) | ✓ |
//! | NetworkLocator URL resolution | ✓ |
//! | Automation (ControlPoints) | TODO |
//! | MIDI tracks | TODO |
//! | Nested compositions | TODO |
//! | Write / export | ✓ |
//!
//! # Example
//!
//! ```no_run
//! // Read
//! let session = dawfile_aaf::read_session("session.aaf")?;
//!
//! println!("Sample rate: {} Hz", session.session_sample_rate);
//! for track in &session.tracks {
//!     println!("  Track {:>3}: {} ({} clips at {})",
//!         track.slot_id, track.name, track.clips.len(), track.edit_rate);
//! }
//! for marker in &session.markers {
//!     println!("  Marker @ {}: {}", marker.position, marker.comment);
//! }
//!
//! // Write (round-trip)
//! dawfile_aaf::write_session("output.aaf", &session)?;
//! # Ok::<(), dawfile_aaf::AafError>(())
//! ```
//!
//! # Capability declaration
//!
//! ```rust
//! let support = dawfile_aaf::feature_support();
//! assert!(support.can_read(daw_proto::Capability::Tracks));
//! assert!(support.can_write(daw_proto::Capability::Tracks));
//! ```

#![deny(unsafe_code)]

pub mod convert;
pub mod error;
pub mod io;
pub mod parse;
pub mod types;
pub mod write;

// ── Public API ────────────────────────────────────────────────────────────────

pub use convert::{feature_support, session_summary};
pub use error::{AafError, AafResult};
pub use io::read_session;
pub use types::{
    AafClip, AafComposition, AafMarker, AafSession, AafTimecode, AafTrack, AudioEssenceInfo,
    ClipKind, EditRate, TrackKind,
};
pub use write::write_session;

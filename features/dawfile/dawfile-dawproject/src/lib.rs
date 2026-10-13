// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! DawProject file parser (`.dawproject`) — pure Rust, cross-platform.
//!
//! Parses the [DawProject](https://github.com/bitwig/dawproject) open exchange
//! format, an open standard for transferring complete music production data
//! between DAWs. Supported by Bitwig Studio 5+, Cubase 14+, Studio One 6.5+.
//!
//! A `.dawproject` file is a ZIP archive containing:
//! - `project.xml` — tracks, clips, arrangement, automation, devices
//! - `metadata.xml` — optional project metadata (title, artist, album, …)
//! - Optional embedded media files referenced by clips
//!
//! # Supported content
//!
//! | Feature | Status |
//! |---|---|
//! | Transport (tempo, time signature) | Supported |
//! | Track hierarchy (audio, MIDI, group, master, effects) | Supported |
//! | Mixer state (volume, pan, mute, sends) | Supported |
//! | Arrangement clips (audio + MIDI) | Supported |
//! | MIDI notes | Supported |
//! | Audio file references and warp markers | Supported |
//! | Automation lanes | Supported |
//! | Markers | Supported |
//! | Scenes / clip launcher | Supported |
//! | Plugin chains (VST2/3, CLAP, AU, built-in) | Supported (metadata only) |
//! | Plugin state blobs | Supported (raw bytes) |
//! | Project metadata | Supported |
//!
//! # Example
//!
//! ```no_run
//! let project = dawfile_dawproject::read_project("session.dawproject")?;
//!
//! println!("Version: {}", project.version);
//! println!("Tempo: {:.1} BPM", project.transport.tempo);
//! println!(
//!     "Time signature: {}/{}",
//!     project.transport.numerator,
//!     project.transport.denominator,
//! );
//!
//! for track in &project.tracks {
//!     println!("Track: {} ({:?})", track.name, track.content_types);
//! }
//! # Ok::<(), dawfile_dawproject::DawProjectError>(())
//! ```
//!
//! # Capability declaration
//!
//! ```rust
//! let support = dawfile_dawproject::feature_support();
//! assert!(support.can_read(daw_proto::Capability::Tracks));
//! assert!(support.can_read(daw_proto::Capability::Midi));
//! assert!(support.can_write(daw_proto::Capability::Tracks)); // read-write
//! ```

#![deny(unsafe_code)]

pub mod convert;
pub mod error;
pub mod io;
pub mod parse;
pub mod types;
pub mod write;

// Re-export the primary public API
pub use convert::feature_support;
pub use error::{DawProjectError, DawProjectResult};
pub use io::{parse_project_bytes, read_project, serialize_project_bytes, write_project};
pub use types::*;

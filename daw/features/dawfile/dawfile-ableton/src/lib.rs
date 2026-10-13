// Lint debt: workspace flipped dead_code/unused to warn (task cleanup);
// this crate predates that — burn down separately.
#![allow(dead_code, unused)]

//! Ableton Live set file parser and writer (.als, .adg) — pure Rust, cross-platform.
//!
//! This crate reads and writes Ableton Live set files (gzipped XML):
//!
//! **Read:**
//! - Project metadata (version, tempo, time signature, key signature)
//! - Audio and MIDI tracks with mixer state (volume, pan, sends)
//! - Return (send) and group tracks with per-track automation
//! - Audio clips with sample references, warp markers, and fades
//! - MIDI clips with note data (per-key tracks, velocity, probability)
//! - Devices/plugins (VST2, VST3, AU, Max for Live, built-in) with recursive rack traversal
//! - Arrangement locators (markers), session view scenes
//! - Transport state, follow actions, clip automation
//! - Tempo automation
//!
//! **Write:**
//! - Full .als generation from `AbletonLiveSet` types
//! - Targets Ableton Live 12 format (compatible with Live 11+)
//! - Gzip compression
//!
//! # Supported formats
//!
//! | Extension | Ableton Version | Read | Write |
//! |-----------|-----------------|------|-------|
//! | `.als`    | 8-12+           | Yes  | Yes   |
//! | `.adg`    | 8-12+           | Yes  | Yes   |
//! | `.alc`    | 8-12+           | Yes  | Yes   |
//!
//! # Example — Read
//!
//! ```no_run
//! let set = dawfile_ableton::read_live_set("project.als")?;
//!
//! println!("Ableton Live {}", set.version);
//! println!("Tempo: {:.1} BPM", set.tempo);
//!
//! for track in &set.audio_tracks {
//!     println!("Audio: {} ({} clips)", track.common.effective_name, track.arrangement_clips.len());
//! }
//! # Ok::<(), dawfile_ableton::AbletonError>(())
//! ```
//!
//! # Example — Write
//!
//! ```no_run
//! let set = dawfile_ableton::read_live_set("input.als")?;
//! // ... modify set ...
//! dawfile_ableton::write_live_set(&set, "output.als")?;
//! # Ok::<(), dawfile_ableton::AbletonError>(())
//! ```

#![deny(unsafe_code)]

pub mod convert;
pub mod devices;
pub mod error;
pub mod io;
pub mod parse;
pub mod types;
pub mod write;

// Re-export the primary public API
pub use convert::feature_support;
pub use error::{AbletonError, AbletonResult};
pub use io::{parse_live_set_bytes, read_live_set};
pub use types::*;
pub use write::{serialize_to_xml, write_live_set, write_live_set_bytes};

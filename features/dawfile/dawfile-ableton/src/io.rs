//! File I/O entry points.
//!
//! High-level functions for reading Ableton Live set files.

use crate::error::{AbletonError, AbletonResult};
use crate::parse;
use crate::types::AbletonLiveSet;
use std::path::Path;

/// Read and parse an Ableton Live set file (.als).
///
/// Handles gzip decompression and XML parsing. Supports Ableton Live 8-12+.
///
/// # Example
///
/// ```no_run
/// let set = dawfile_ableton::read_live_set("project.als")?;
/// println!("Version: {}", set.version);
/// println!("Tempo: {:.1} BPM", set.tempo);
/// println!("Audio tracks: {}", set.audio_tracks.len());
/// println!("MIDI tracks: {}", set.midi_tracks.len());
/// # Ok::<(), dawfile_ableton::AbletonError>(())
/// ```
pub fn read_live_set(path: impl AsRef<Path>) -> AbletonResult<AbletonLiveSet> {
    let data = std::fs::read(path.as_ref())?;
    parse_live_set_bytes(&data)
}

/// Parse an Ableton Live set from raw bytes (gzipped .als content).
///
/// Useful when the data is already in memory (e.g., from a network stream).
pub fn parse_live_set_bytes(data: &[u8]) -> AbletonResult<AbletonLiveSet> {
    let xml = decompress(data)?;
    let xml_str =
        String::from_utf8(xml).map_err(|e| AbletonError::Xml(format!("invalid UTF-8: {e}")))?;
    parse::parse_live_set(&xml_str)
}

/// Decompress gzip data. Returns the raw XML bytes.
fn decompress(data: &[u8]) -> AbletonResult<Vec<u8>> {
    use flate2::read::GzDecoder;
    use std::io::Read;

    // Detect pre-v8 Ableton binary format
    if data.len() >= 2 && data[0] == 0xAB && data[1] == 0x1E {
        return Err(AbletonError::PreVersion8);
    }

    // Verify gzip magic bytes
    if data.len() < 2 || data[0] != 0x1f || data[1] != 0x8b {
        return Err(AbletonError::NotGzip);
    }

    let mut decoder = GzDecoder::new(data);
    let mut xml = Vec::new();
    decoder.read_to_end(&mut xml)?;
    Ok(xml)
}

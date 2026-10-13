//! Pro Tools session metadata extraction and stripping
//!
//! Handles Pro Tools-specific naming conventions including:
//! - Playlist markers: `_01`, `_02`, `.01`, `.02`
//! - Take markers: `_01-01`, `_02-03` (playlist-take format)
//! - Region markers: `.01_01`, `.02_01`
//!
//! Pro Tools uses these suffixes to track different playlist versions and takes
//! within a session. These are administrative metadata, not content descriptors,
//! and should be stripped from display names.

use std::borrow::Cow;

/// Information extracted from Pro Tools naming conventions
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProToolsMetadata {
    /// Playlist number (1-indexed, from `_01`, `.01`, etc.)
    pub playlist: Option<u32>,
    /// Take number within the playlist (from `_01-02` format)
    pub take: Option<u32>,
    /// Region number (from `.01_01` format)
    pub region: Option<u32>,
}

impl ProToolsMetadata {
    /// Returns true if any Pro Tools metadata was extracted
    #[must_use]
    pub const fn has_metadata(&self) -> bool {
        self.playlist.is_some() || self.take.is_some() || self.region.is_some()
    }
}

/// Extract Pro Tools metadata from a string
///
/// Looks for patterns like:
/// - `Track_01.wav` → playlist 1
/// - `Track.01.wav` → playlist 1
/// - `Track_01-02.wav` → playlist 1, take 2
/// - `Track.01_01.wav` → playlist 1, region 1
///
/// # Examples
///
/// ```
/// use dynamic_template::protools::extract_protools_metadata;
///
/// let meta = extract_protools_metadata("Guitar_01.wav");
/// assert_eq!(meta.playlist, Some(1));
///
/// let meta = extract_protools_metadata("Vocal_02-03.wav");
/// assert_eq!(meta.playlist, Some(2));
/// assert_eq!(meta.take, Some(3));
/// ```
#[must_use]
pub fn extract_protools_metadata(input: &str) -> ProToolsMetadata {
    let mut metadata = ProToolsMetadata::default();

    // Remove file extension for analysis
    let name = strip_extension(input);

    // Try to match playlist-take format: _01-02 or _01-03
    if let Some((playlist, take)) = extract_playlist_take(name) {
        metadata.playlist = Some(playlist);
        metadata.take = Some(take);
        return metadata;
    }

    // Try to match region format: .01_01 or .02_03
    if let Some((playlist, region)) = extract_playlist_region(name) {
        metadata.playlist = Some(playlist);
        metadata.region = Some(region);
        return metadata;
    }

    // Try to match simple playlist format: _01 or .01 at end
    if let Some(playlist) = extract_simple_playlist(name) {
        metadata.playlist = Some(playlist);
        return metadata;
    }

    metadata
}

/// Strip Pro Tools playlist/take markers from a string
///
/// Removes patterns like:
/// - `_01`, `_02`, `_03` (playlist markers)
/// - `.01`, `.02`, `.03` (alternate playlist markers)
/// - `_01-01`, `_02-03` (playlist-take markers)
/// - `.01_01`, `.02_01` (playlist-region markers)
///
/// # Examples
///
/// ```
/// use dynamic_template::protools::strip_protools_markers;
///
/// assert_eq!(strip_protools_markers("Guitar_01.wav"), "Guitar.wav");
/// assert_eq!(strip_protools_markers("Vocal.01.wav"), "Vocal.wav");
/// assert_eq!(strip_protools_markers("Bass_02-03.wav"), "Bass.wav");
/// assert_eq!(strip_protools_markers("Drums.01_02.wav"), "Drums.wav");
/// ```
#[must_use]
pub fn strip_protools_markers(input: &str) -> String {
    // Handle file extension
    let (name, ext) = split_extension(input);

    // Try stripping patterns in order of specificity
    let stripped = strip_playlist_take(name)
        .or_else(|| strip_playlist_region(name))
        .or_else(|| strip_simple_playlist(name))
        .unwrap_or(Cow::Borrowed(name));

    // Reconstruct with extension
    if ext.is_empty() {
        stripped.into_owned()
    } else {
        format!("{stripped}.{ext}")
    }
}

/// Check if a string contains Pro Tools markers
///
/// Returns true if the string contains patterns that look like Pro Tools
/// playlist or take markers.
#[must_use]
pub fn has_protools_markers(input: &str) -> bool {
    let name = strip_extension(input);
    extract_simple_playlist(name).is_some()
        || extract_playlist_take(name).is_some()
        || extract_playlist_region(name).is_some()
}

// --- Helper functions ---

fn strip_extension(input: &str) -> &str {
    // Find last dot that's preceded by something and followed by 2-4 chars
    if let Some(dot_pos) = input.rfind('.') {
        let ext = input.get(dot_pos.saturating_add(1)..).unwrap_or_default();
        if ext.len() >= 2 && ext.len() <= 4 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
            return input.get(..dot_pos).unwrap_or_default();
        }
    }
    input
}

fn split_extension(input: &str) -> (&str, &str) {
    if let Some(dot_pos) = input.rfind('.') {
        let ext = input.get(dot_pos.saturating_add(1)..).unwrap_or_default();
        if ext.len() >= 2 && ext.len() <= 4 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
            return (input.get(..dot_pos).unwrap_or_default(), ext);
        }
    }
    (input, "")
}

/// Extract simple playlist number from end of string
/// Matches: `_01`, `_02`, `.01`, `.02` at the end
fn extract_simple_playlist(name: &str) -> Option<u32> {
    let bytes = name.as_bytes();
    let len = bytes.len();

    // Need at least 3 chars: separator + 2 digits
    if len < 3 {
        return None;
    }

    // Check for _NN or .NN at end
    let sep = bytes.get(len.saturating_sub(3)).copied().unwrap_or(0);
    if sep != b'_' && sep != b'.' {
        return None;
    }

    // Check that the last two chars are digits
    let d1 = bytes.get(len.saturating_sub(2)).copied().unwrap_or(0);
    let d2 = bytes.get(len.saturating_sub(1)).copied().unwrap_or(0);
    if !d1.is_ascii_digit() || !d2.is_ascii_digit() {
        return None;
    }

    // Check that we're not in the middle of a longer pattern
    // e.g., don't match _01 in _01-02
    // Already at end, so this is fine

    // Parse the number
    let num_str = &name.get(len.saturating_sub(2)..).unwrap_or_default();
    num_str.parse::<u32>().ok()
}

/// Extract playlist-take format: _01-02, _01-03
fn extract_playlist_take(name: &str) -> Option<(u32, u32)> {
    // Look for pattern: _NN-NN at end
    let bytes = name.as_bytes();
    let len = bytes.len();

    // Need at least 6 chars: _01-01
    if len < 6 {
        return None;
    }

    // Check for pattern _DD-DD at end
    if bytes.get(len.saturating_sub(6)).copied().unwrap_or(0) != b'_' {
        return None;
    }
    if bytes.get(len.saturating_sub(3)).copied().unwrap_or(0) != b'-' {
        return None;
    }

    // Check digits
    if !bytes
        .get(len.saturating_sub(5))
        .copied()
        .unwrap_or(0)
        .is_ascii_digit()
        || !bytes
            .get(len.saturating_sub(4))
            .copied()
            .unwrap_or(0)
            .is_ascii_digit()
        || !bytes
            .get(len.saturating_sub(2))
            .copied()
            .unwrap_or(0)
            .is_ascii_digit()
        || !bytes
            .get(len.saturating_sub(1))
            .copied()
            .unwrap_or(0)
            .is_ascii_digit()
    {
        return None;
    }

    let playlist = name
        .get(len.saturating_sub(5)..len.saturating_sub(3))
        .unwrap_or_default()
        .parse::<u32>()
        .ok()?;
    let take = name
        .get(len.saturating_sub(2)..)
        .unwrap_or_default()
        .parse::<u32>()
        .ok()?;

    Some((playlist, take))
}

/// Extract playlist-region format: .`01_01`, .`02_03`
fn extract_playlist_region(name: &str) -> Option<(u32, u32)> {
    // Look for pattern: .NN_NN at end
    let bytes = name.as_bytes();
    let len = bytes.len();

    // Need at least 6 chars: .01_01
    if len < 6 {
        return None;
    }

    // Check for pattern .DD_DD at end
    if bytes.get(len.saturating_sub(6)).copied().unwrap_or(0) != b'.' {
        return None;
    }
    if bytes.get(len.saturating_sub(3)).copied().unwrap_or(0) != b'_' {
        return None;
    }

    // Check digits
    if !bytes
        .get(len.saturating_sub(5))
        .copied()
        .unwrap_or(0)
        .is_ascii_digit()
        || !bytes
            .get(len.saturating_sub(4))
            .copied()
            .unwrap_or(0)
            .is_ascii_digit()
        || !bytes
            .get(len.saturating_sub(2))
            .copied()
            .unwrap_or(0)
            .is_ascii_digit()
        || !bytes
            .get(len.saturating_sub(1))
            .copied()
            .unwrap_or(0)
            .is_ascii_digit()
    {
        return None;
    }

    let playlist = name
        .get(len.saturating_sub(5)..len.saturating_sub(3))
        .unwrap_or_default()
        .parse::<u32>()
        .ok()?;
    let region = name
        .get(len.saturating_sub(2)..)
        .unwrap_or_default()
        .parse::<u32>()
        .ok()?;

    Some((playlist, region))
}

/// Strip simple playlist marker from end
fn strip_simple_playlist(name: &str) -> Option<Cow<'_, str>> {
    let bytes = name.as_bytes();
    let len = bytes.len();

    if len < 3 {
        return None;
    }

    let sep = bytes.get(len.saturating_sub(3)).copied().unwrap_or(0);
    if sep != b'_' && sep != b'.' {
        return None;
    }

    let d1 = bytes.get(len.saturating_sub(2)).copied().unwrap_or(0);
    let d2 = bytes.get(len.saturating_sub(1)).copied().unwrap_or(0);
    if !d1.is_ascii_digit() || !d2.is_ascii_digit() {
        return None;
    }

    // Don't strip if this looks like a track number at the start
    // e.g., "01.Kick" should keep the number
    // But "Kick_01" should strip the playlist marker
    // Heuristic: if the separator is at position 2 and prefix is all digits, keep it
    if len == 3 && sep == b'.' {
        // This is just "NN." which could be a track number prefix
        // Don't strip in this case
        return None;
    }

    Some(Cow::Borrowed(
        name.get(..len.saturating_sub(3)).unwrap_or_default(),
    ))
}

/// Strip playlist-take marker from end
fn strip_playlist_take(name: &str) -> Option<Cow<'_, str>> {
    let bytes = name.as_bytes();
    let len = bytes.len();

    if len < 6 {
        return None;
    }

    if bytes.get(len.saturating_sub(6)).copied().unwrap_or(0) != b'_' {
        return None;
    }
    if bytes.get(len.saturating_sub(3)).copied().unwrap_or(0) != b'-' {
        return None;
    }

    if !bytes
        .get(len.saturating_sub(5))
        .copied()
        .unwrap_or(0)
        .is_ascii_digit()
        || !bytes
            .get(len.saturating_sub(4))
            .copied()
            .unwrap_or(0)
            .is_ascii_digit()
        || !bytes
            .get(len.saturating_sub(2))
            .copied()
            .unwrap_or(0)
            .is_ascii_digit()
        || !bytes
            .get(len.saturating_sub(1))
            .copied()
            .unwrap_or(0)
            .is_ascii_digit()
    {
        return None;
    }

    Some(Cow::Borrowed(
        name.get(..len.saturating_sub(6)).unwrap_or_default(),
    ))
}

/// Strip playlist-region marker from end
fn strip_playlist_region(name: &str) -> Option<Cow<'_, str>> {
    let bytes = name.as_bytes();
    let len = bytes.len();

    if len < 6 {
        return None;
    }

    if bytes.get(len.saturating_sub(6)).copied().unwrap_or(0) != b'.' {
        return None;
    }
    if bytes.get(len.saturating_sub(3)).copied().unwrap_or(0) != b'_' {
        return None;
    }

    if !bytes
        .get(len.saturating_sub(5))
        .copied()
        .unwrap_or(0)
        .is_ascii_digit()
        || !bytes
            .get(len.saturating_sub(4))
            .copied()
            .unwrap_or(0)
            .is_ascii_digit()
        || !bytes
            .get(len.saturating_sub(2))
            .copied()
            .unwrap_or(0)
            .is_ascii_digit()
        || !bytes
            .get(len.saturating_sub(1))
            .copied()
            .unwrap_or(0)
            .is_ascii_digit()
    {
        return None;
    }

    Some(Cow::Borrowed(
        name.get(..len.saturating_sub(6)).unwrap_or_default(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Extraction tests ---

    #[test]
    fn extract_simple_playlist_underscore() {
        let meta = extract_protools_metadata("Guitar_01.wav");
        assert_eq!(meta.playlist, Some(1));
        assert_eq!(meta.take, None);
        assert_eq!(meta.region, None);
    }

    #[test]
    fn extract_simple_playlist_dot() {
        let meta = extract_protools_metadata("Guitar.01.wav");
        assert_eq!(meta.playlist, Some(1));
    }

    #[test]
    fn extract_playlist_take_format() {
        let meta = extract_protools_metadata("Vocal_02-03.wav");
        assert_eq!(meta.playlist, Some(2));
        assert_eq!(meta.take, Some(3));
    }

    #[test]
    fn extract_playlist_region_format() {
        let meta = extract_protools_metadata("Drums.01_02.wav");
        assert_eq!(meta.playlist, Some(1));
        assert_eq!(meta.region, Some(2));
    }

    #[test]
    fn no_protools_markers() {
        let meta = extract_protools_metadata("Just A Track Name.wav");
        assert!(!meta.has_metadata());
    }

    #[test]
    fn dont_match_track_numbers() {
        // Track numbers like "01.Kick" should not be treated as playlist markers
        let meta = extract_protools_metadata("01.Kick_01.wav");
        // Should match the _01 at the end, not the 01. at the start
        assert_eq!(meta.playlist, Some(1));
    }

    // --- Stripping tests ---

    #[test]
    fn strip_simple_underscore_playlist() {
        assert_eq!(strip_protools_markers("Guitar_01.wav"), "Guitar.wav");
        assert_eq!(strip_protools_markers("Bass_02.wav"), "Bass.wav");
        assert_eq!(strip_protools_markers("Drums_10.wav"), "Drums.wav");
    }

    #[test]
    fn strip_simple_dot_playlist() {
        assert_eq!(strip_protools_markers("Guitar.01.wav"), "Guitar.wav");
        assert_eq!(strip_protools_markers("Bass.02.wav"), "Bass.wav");
    }

    #[test]
    fn strip_playlist_take() {
        assert_eq!(strip_protools_markers("Vocal_01-02.wav"), "Vocal.wav");
        assert_eq!(strip_protools_markers("Lead_03-05.wav"), "Lead.wav");
    }

    #[test]
    fn strip_playlist_region() {
        assert_eq!(strip_protools_markers("Drums.01_01.wav"), "Drums.wav");
        assert_eq!(strip_protools_markers("Bass.02_03.wav"), "Bass.wav");
    }

    #[test]
    fn preserve_track_number_prefix() {
        // "01.Kick" is a track number, not a playlist marker
        // The _01 at the end IS a playlist marker
        assert_eq!(strip_protools_markers("01.Kick_01.wav"), "01.Kick.wav");
    }

    #[test]
    fn no_markers_to_strip() {
        assert_eq!(
            strip_protools_markers("Just A Track.wav"),
            "Just A Track.wav"
        );
        assert_eq!(strip_protools_markers("Kick In.wav"), "Kick In.wav");
    }

    #[test]
    fn without_extension() {
        assert_eq!(strip_protools_markers("Guitar_01"), "Guitar");
        assert_eq!(strip_protools_markers("Vocal_02-03"), "Vocal");
    }

    #[test]
    fn real_world_examples() {
        // From multitrack tests
        assert_eq!(
            strip_protools_markers("H3000.One_01-01.wav"),
            "H3000.One.wav"
        );
        assert_eq!(
            strip_protools_markers("Breathy Synth 1_01-02.wav"),
            "Breathy Synth 1.wav"
        );
        assert_eq!(
            strip_protools_markers("Vocal.Eko.Plate_01.wav"),
            "Vocal.Eko.Plate.wav"
        );
    }

    #[test]
    fn edge_cases() {
        // Very short names
        assert_eq!(strip_protools_markers("A_01.wav"), "A.wav");

        // Multiple underscores
        assert_eq!(
            strip_protools_markers("Lead_Guitar_Clean_01.wav"),
            "Lead_Guitar_Clean.wav"
        );

        // Dots in name
        assert_eq!(
            strip_protools_markers("H3000.One.New_01.wav"),
            "H3000.One.New.wav"
        );
    }
}

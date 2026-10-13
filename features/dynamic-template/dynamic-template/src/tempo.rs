//! Tempo extraction utilities
//!
//! Extracts tempo/BPM values from track names like "126bpm", "83.5BPM", etc.

/// Strip tempo/BPM from a string
///
/// Removes patterns like "126bpm", "83.5BPM", etc. from the string
/// and cleans up surrounding punctuation.
///
/// # Examples
///
/// ```
/// use dynamic_template::strip_tempo;
///
/// assert_eq!(strip_tempo("Track.126bpm.wav"), "Track.wav");
/// assert_eq!(strip_tempo("83.5BPM_song"), "song");
/// assert_eq!(strip_tempo("AD.Big Kit.83.5BPM.L.wav"), "AD.Big Kit.L.wav");
/// ```
#[must_use]
pub fn strip_tempo(input: &str) -> String {
    let lower = input.to_lowercase();

    // Find "bpm" in the string
    let Some(bpm_idx) = lower.find("bpm") else {
        return input.to_string();
    };

    let bpm_end = bpm_idx.saturating_add(3); // length of "bpm"

    // Look backwards from bpm to find the start of the number
    let prefix = input.get(..bpm_idx).unwrap_or_default();
    let prefix_trimmed = prefix.trim_end();

    let mut num_start = prefix_trimmed.len();
    let mut found_digit = false;
    let mut has_decimal = false;

    for (i, c) in prefix_trimmed.char_indices().rev() {
        if c.is_ascii_digit() {
            num_start = i;
            found_digit = true;
        } else if c == '.' && found_digit && !has_decimal {
            num_start = i;
            has_decimal = true;
        } else if found_digit {
            break;
        }
    }

    if !found_digit {
        return input.to_string();
    }

    // Build the result by removing the tempo pattern
    let before = input.get(..num_start).unwrap_or_default();
    let after = input.get(bpm_end..).unwrap_or_default();

    // Clean up separators around the removed portion
    let before = before.trim_end_matches(['.', '_', ' ', '-']);
    let after = after.trim_start_matches(['.', '_', ' ', '-']);

    if before.is_empty() {
        after.to_string()
    } else if after.is_empty() {
        before.to_string()
    } else {
        // Try to preserve the original separator style by looking at what was around the tempo
        // Look at the character immediately before the number in the original string
        let sep_before = num_start
            .checked_sub(1)
            .and_then(|idx| input.chars().nth(idx));

        let separator = match sep_before {
            Some('_') => "_",
            Some('-') => "-",
            Some(' ') => " ",
            _ => ".",
        };
        format!("{before}{separator}{after}")
    }
}

/// Extract tempo in BPM from a string
///
/// Looks for patterns like:
/// - "126bpm"
/// - "83.5BPM"
/// - "`120_bpm`"
/// - "140 BPM"
///
/// Returns the tempo as f32 if found, None otherwise.
///
/// # Examples
///
/// ```
/// use dynamic_template::extract_tempo;
///
/// assert_eq!(extract_tempo("Track.126bpm.wav"), Some(126.0));
/// assert_eq!(extract_tempo("83.5BPM_song"), Some(83.5));
/// assert_eq!(extract_tempo("No tempo here"), None);
/// ```
#[must_use]
pub fn extract_tempo(input: &str) -> Option<f32> {
    let lower = input.to_lowercase();

    // Find "bpm" in the string
    let bpm_idx = lower.find("bpm")?;

    // Look backwards from bpm to find the number
    let prefix = input.get(..bpm_idx).unwrap_or_default();

    // Find where the number ends (right before bpm or any separator before bpm)
    let prefix_trimmed = prefix.trim_end();

    // Find where the number starts by scanning backwards
    let mut num_start = prefix_trimmed.len();
    let mut num_end = prefix_trimmed.len();
    let mut found_digit = false;
    let mut has_decimal = false;

    for (i, c) in prefix_trimmed.char_indices().rev() {
        if c.is_ascii_digit() {
            num_start = i;
            if !found_digit {
                num_end = i.saturating_add(c.len_utf8());
            }
            found_digit = true;
        } else if c == '.' && found_digit && !has_decimal {
            // Allow one decimal point within the number
            num_start = i;
            has_decimal = true;
        } else if found_digit {
            // We've hit a non-digit after finding digits, stop
            break;
        }
        // Continue scanning backwards past separators when we haven't found digits yet
    }

    if !found_digit {
        return None;
    }

    // Extract the number substring
    let num_str = prefix_trimmed.get(num_start..num_end).unwrap_or_default();

    // Handle leading decimal point (e.g., ".5bpm" -> not valid)
    let num_str = num_str.trim_start_matches(|c: char| !c.is_ascii_digit());

    // Parse as f32
    num_str.parse::<f32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_integer_tempo() {
        assert_eq!(extract_tempo("126bpm"), Some(126.0));
        assert_eq!(extract_tempo("120BPM"), Some(120.0));
        assert_eq!(extract_tempo("140 bpm"), Some(140.0));
        assert_eq!(extract_tempo("85_bpm"), Some(85.0));
    }

    #[test]
    fn extract_decimal_tempo() {
        assert_eq!(extract_tempo("83.5bpm"), Some(83.5));
        assert_eq!(extract_tempo("127.4BPM"), Some(127.4));
        assert_eq!(extract_tempo("99.99bpm"), Some(99.99));
    }

    #[test]
    fn extract_tempo_in_filename() {
        assert_eq!(
            extract_tempo("01.LV BECCA.TimeAfterTime.126bpm"),
            Some(126.0)
        );
        assert_eq!(extract_tempo("Track_83.5BPM_v2.wav"), Some(83.5));
        assert_eq!(extract_tempo("Song.120bpm.stem.wav"), Some(120.0));
    }

    #[test]
    fn no_tempo_found() {
        assert_eq!(extract_tempo("Kick In.wav"), None);
        assert_eq!(extract_tempo("bpm"), None); // Just "bpm" without number
        assert_eq!(extract_tempo("Vocal Track"), None);
    }

    #[test]
    fn edge_cases() {
        // Number right before bpm
        assert_eq!(extract_tempo("abc123bpm"), Some(123.0));
        // Decimal at end
        assert_eq!(extract_tempo("120.bpm"), Some(120.0));
    }

    // Tests for strip_tempo

    #[test]
    fn strip_tempo_basic() {
        assert_eq!(strip_tempo("126bpm"), "");
        assert_eq!(strip_tempo("Track.126bpm"), "Track");
        assert_eq!(strip_tempo("Track.126bpm.wav"), "Track.wav");
    }

    #[test]
    fn strip_tempo_decimal() {
        assert_eq!(strip_tempo("83.5BPM_song"), "song");
        assert_eq!(strip_tempo("AD.Big Kit.83.5BPM.L.wav"), "AD.Big Kit.L.wav");
        assert_eq!(strip_tempo("Vocal.Comp.83.5BPM.wav"), "Vocal.Comp.wav");
    }

    #[test]
    fn strip_tempo_preserves_separators() {
        assert_eq!(strip_tempo("Track_120bpm_v2"), "Track_v2");
        assert_eq!(strip_tempo("Audio.120bpm.stem"), "Audio.stem");
    }

    #[test]
    fn strip_tempo_no_tempo() {
        assert_eq!(strip_tempo("Kick In.wav"), "Kick In.wav");
        assert_eq!(strip_tempo("Vocal Track"), "Vocal Track");
        assert_eq!(strip_tempo("Bass DI"), "Bass DI");
    }

    #[test]
    fn strip_tempo_real_world() {
        // From steve_maggiora_hey_lady test
        assert_eq!(
            strip_tempo("AD.Big Kit Gretch.83.5BPM.L"),
            "AD.Big Kit Gretch.L"
        );
        assert_eq!(strip_tempo("Bass.Di.83.5BPM"), "Bass.Di");
        assert_eq!(strip_tempo("H3000.One.New.83.5BPM.L"), "H3000.One.New.L");
    }
}

//! Equipment detection and stripping
//!
//! Handles detection and removal of equipment-related metadata from track names:
//! - Microphone models (U47, SM57, C414, etc.)
//! - Preamp/compressor models (`OptoComp`, LA2A, 1176, etc.)
//! - Processing markers (LIM, EDT, etc.)
//! - Synth/keyboard hardware (CASIO, Roland FA06, Nord, etc.)
//!
//! This metadata is useful for documentation but clutters display names.

/// Known microphone models and their variations
///
/// These are common microphone models used in professional recording.
/// Format: (pattern, `canonical_name`) - `canonical_name` is for potential future use
pub const MIC_MODELS: &[(&str, &str)] = &[
    // Neumann
    ("u47", "Neumann U47"),
    ("u67", "Neumann U67"),
    ("u87", "Neumann U87"),
    ("u89", "Neumann U89"),
    ("m49", "Neumann M49"),
    ("m50", "Neumann M50"),
    ("km84", "Neumann KM84"),
    ("km184", "Neumann KM184"),
    ("tlm103", "Neumann TLM103"),
    ("tlm170", "Neumann TLM170"),
    ("neumann", "Neumann"),
    ("47x", "Neumann U47"),         // Common abbreviation
    ("67x", "Neumann U67"),         // Common abbreviation
    ("forty.seven", "Neumann U47"), // Stylized
    ("forty seven", "Neumann U47"),
    // Shure
    ("sm57", "Shure SM57"),
    ("sm58", "Shure SM58"),
    ("sm7", "Shure SM7"),
    ("sm7b", "Shure SM7B"),
    ("beta52", "Shure Beta 52"),
    ("beta91", "Shure Beta 91"),
    ("ksm32", "Shure KSM32"),
    ("ksm44", "Shure KSM44"),
    ("57", "Shure SM57"), // Common shorthand in session
    ("58", "Shure SM58"),
    // Sennheiser
    ("md421", "Sennheiser MD421"),
    ("md441", "Sennheiser MD441"),
    ("e602", "Sennheiser e602"),
    ("e604", "Sennheiser e604"),
    ("e609", "Sennheiser e609"),
    ("421", "Sennheiser MD421"), // Common shorthand
    ("441", "Sennheiser MD441"),
    // AKG
    ("c414", "AKG C414"),
    ("c12", "AKG C12"),
    ("d112", "AKG D112"),
    ("c451", "AKG C451"),
    ("414", "AKG C414"), // Common shorthand
    ("112", "AKG D112"),
    // Royer
    ("r121", "Royer R-121"),
    ("r122", "Royer R-122"),
    ("sf12", "Royer SF-12"),
    ("121", "Royer R-121"), // Common shorthand
    ("royer", "Royer"),
    // RCA/Ribbon
    ("rca44", "RCA 44"),
    ("rca77", "RCA 77"),
    ("44a", "RCA 44"), // Common shorthand
    ("77dx", "RCA 77"),
    // AEA
    ("aea r84", "AEA R84"),
    ("r84", "AEA R84"),
    ("aea", "AEA"),
    // Telefunken
    ("ela m251", "Telefunken ELA M 251"),
    ("m251", "Telefunken M251"),
    ("telefunken", "Telefunken"),
    // Coles
    ("coles 4038", "Coles 4038"),
    ("4038", "Coles 4038"),
    // Electro-Voice
    ("re20", "Electro-Voice RE20"),
    ("re320", "Electro-Voice RE320"),
    // Audio-Technica
    ("at4050", "Audio-Technica AT4050"),
    ("at4033", "Audio-Technica AT4033"),
    // Beyerdynamic
    ("m160", "Beyerdynamic M160"),
    ("m88", "Beyerdynamic M88"),
    // Sony
    ("c800g", "Sony C-800G"),
    // Earthworks
    ("sr25", "Earthworks SR25"),
    // DPA
    ("dpa 4011", "DPA 4011"),
    ("4011", "DPA 4011"),
    // Crown
    ("pzm", "Crown PZM"),
    // Rode
    ("nt1", "Rode NT1"),
    ("nt5", "Rode NT5"),
    ("ntk", "Rode NTK"),
];

/// Known preamp and compressor models
pub const PREAMP_MODELS: &[(&str, &str)] = &[
    // Universal Audio / UREI
    ("1176", "UREI 1176"),
    ("la2a", "Teletronix LA-2A"),
    ("la3a", "UREI LA-3A"),
    ("610", "UA 610"),
    ("6176", "UA 6176"),
    // API
    ("api 512", "API 512"),
    ("api 2500", "API 2500"),
    ("312", "API 312"),
    ("512", "API 512"),
    // Neve
    ("1073", "Neve 1073"),
    ("1084", "Neve 1084"),
    ("33609", "Neve 33609"),
    ("neve", "Neve"),
    // SSL
    ("ssl", "SSL"),
    ("g bus", "SSL G Bus"),
    ("g-bus", "SSL G Bus"),
    // Avalon
    ("avalon", "Avalon"),
    ("vt737", "Avalon VT-737"),
    // dbx
    ("dbx160", "dbx 160"),
    ("dbx165", "dbx 165"),
    // Empirical Labs
    ("distressor", "Empirical Labs Distressor"),
    ("fatso", "Empirical Labs Fatso"),
    // Manley
    ("manley", "Manley"),
    ("voxbox", "Manley Voxbox"),
    // Chandler
    ("chandler", "Chandler"),
    // Shadow Hills
    ("shadow hills", "Shadow Hills"),
    // BAE
    ("bae", "BAE"),
    // Warm Audio
    ("wa73", "Warm Audio WA73"),
    // Focusrite
    ("isa", "Focusrite ISA"),
    // Grace Design - use full name to avoid matching common word "Grace"
    ("grace design", "Grace Design"),
    ("grace preamp", "Grace Design"),
    // Great River
    ("mp500", "Great River MP-500"),
    // Tube-Tech
    ("cl1b", "Tube-Tech CL 1B"),
    // Thermionic Culture
    ("culture vulture", "Thermionic Culture Vulture"),
    // Meters
    ("260vu", "260 VU Meter"),
    ("vu meter", "VU Meter"),
    // Opto compressors
    ("optocomp", "Opto Compressor"),
    ("opto comp", "Opto Compressor"),
];

/// Processing markers that indicate track state but aren't content descriptors
/// Note: These are stripped only when they appear as standalone words or common suffixes
pub const PROCESSING_MARKERS: &[&str] = &[
    "lim", // Limiter applied
    "limit",
    "limited",
    "edt", // Edit marker
    // Note: "edit" and "edited" excluded - too common in valid names
    "no compression",
    "no comp",
    // Note: "compressed" excluded - matches "Compressed Air SFX" incorrectly
    "uncompressed",
    // Note: "raw", "dry", "wet" excluded - could be valid instrument descriptors
    "processed",
    "unprocessed",
    "print", // Printed/committed effect
    "printed",
    "bounce", // Bounced track
    "bounced",
    // Note: "dup" excluded - too short, matches partial words
    "duplicate",
    "mst", // Master
    // Note: "master" excluded - could be valid ("Guitar Master")
    "rough", // Rough mix
    // Note: "final" excluded - could be valid in song section names
    "alt", // Alternate
    "backup",
];

/// Synth and keyboard hardware models
pub const SYNTH_HARDWARE: &[(&str, &str)] = &[
    // Casio
    ("casio", "Casio"),
    ("ctk-601", "Casio CTK-601"),
    ("ctk601", "Casio CTK-601"),
    ("cz-101", "Casio CZ-101"),
    ("cz101", "Casio CZ-101"),
    // Roland
    ("roland", "Roland"),
    ("juno", "Roland Juno"),
    ("jupiter", "Roland Jupiter"),
    ("d-50", "Roland D-50"),
    ("d50", "Roland D-50"),
    ("jv-1080", "Roland JV-1080"),
    ("jv1080", "Roland JV-1080"),
    ("fa-06", "Roland FA-06"),
    ("fa06", "Roland FA-06"),
    ("fantom", "Roland Fantom"),
    // Korg
    ("korg", "Korg"),
    ("m1", "Korg M1"),
    ("triton", "Korg Triton"),
    ("kronos", "Korg Kronos"),
    ("minilogue", "Korg Minilogue"),
    ("prologue", "Korg Prologue"),
    // Yamaha
    ("yamaha", "Yamaha"),
    ("dx7", "Yamaha DX7"),
    ("motif", "Yamaha Motif"),
    ("montage", "Yamaha Montage"),
    ("cs-80", "Yamaha CS-80"),
    ("cs80", "Yamaha CS-80"),
    // Nord
    ("nord", "Nord"),
    ("nord lead", "Nord Lead"),
    ("nord stage", "Nord Stage"),
    // Moog
    ("moog", "Moog"),
    ("minimoog", "Minimoog"),
    ("sub37", "Moog Sub 37"),
    ("sub 37", "Moog Sub 37"),
    ("voyager", "Moog Voyager"),
    ("grandmother", "Moog Grandmother"),
    // Dave Smith / Sequential
    ("prophet", "Sequential Prophet"),
    ("prophet 5", "Sequential Prophet-5"),
    ("prophet-5", "Sequential Prophet-5"),
    ("ob-6", "Sequential OB-6"),
    ("rev2", "Sequential Rev2"),
    // Oberheim
    ("oberheim", "Oberheim"),
    ("ob-x", "Oberheim OB-X"),
    ("obx", "Oberheim OB-X"),
    // Arturia
    ("arturia", "Arturia"),
    ("minibrute", "Arturia MiniBrute"),
    ("matrixbrute", "Arturia MatrixBrute"),
    // Access
    ("virus", "Access Virus"),
    // Waldorf
    ("waldorf", "Waldorf"),
    ("blofeld", "Waldorf Blofeld"),
    // Novation
    ("novation", "Novation"),
    ("bass station", "Novation Bass Station"),
    ("peak", "Novation Peak"),
    // Elektron
    ("elektron", "Elektron"),
    ("analog four", "Elektron Analog Four"),
    ("digitone", "Elektron Digitone"),
    // Teenage Engineering
    ("op-1", "Teenage Engineering OP-1"),
    ("op1", "Teenage Engineering OP-1"),
    // Clavia (Nord brand parent)
    ("clavia", "Clavia"),
    // Hammond / organ brands
    ("hammond", "Hammond"),
    ("leslie", "Leslie"),
    // Settings/presets that should be stripped
    ("briteness", "Brightness Setting"),
    ("brightness", "Brightness Setting"),
    ("charang", "Charang Preset"),
    ("dog bark", "Dog Bark Preset"),
    ("weird synth patch", "Synth Patch"),
];

/// Check if a string contains a mic model
#[must_use]
pub fn contains_mic_model(input: &str) -> bool {
    let lower = input.to_lowercase();
    MIC_MODELS.iter().any(|(pattern, _)| {
        // Check for word boundary match
        contains_word(&lower, pattern)
    })
}

/// Check if a string contains a preamp/compressor model
#[must_use]
pub fn contains_preamp_model(input: &str) -> bool {
    let lower = input.to_lowercase();
    PREAMP_MODELS
        .iter()
        .any(|(pattern, _)| contains_word(&lower, pattern))
}

/// Check if a string contains synth hardware
#[must_use]
pub fn contains_synth_hardware(input: &str) -> bool {
    let lower = input.to_lowercase();
    SYNTH_HARDWARE
        .iter()
        .any(|(pattern, _)| contains_word(&lower, pattern))
}

/// Strip equipment names from a display name
///
/// Removes mic models, preamp models, processing markers, and synth hardware
/// from the string while preserving the core content.
///
/// # Examples
///
/// ```
/// use dynamic_template::equipment::strip_equipment;
///
/// assert_eq!(strip_equipment("Kick OptoComp"), "Kick");
/// assert_eq!(strip_equipment("OH L 260VU"), "OH L");
/// assert_eq!(strip_equipment("Synth - CASIO CTK-601 Briteness"), "Synth");
/// ```
#[must_use]
pub fn strip_equipment(input: &str) -> String {
    let mut result = input.to_string();

    // Strip mic models
    for (pattern, _) in MIC_MODELS {
        result = strip_pattern(&result, pattern);
    }

    // Strip preamp models
    for (pattern, _) in PREAMP_MODELS {
        result = strip_pattern(&result, pattern);
    }

    // Strip processing markers
    for pattern in PROCESSING_MARKERS {
        result = strip_pattern(&result, pattern);
    }

    // Strip synth hardware
    for (pattern, _) in SYNTH_HARDWARE {
        result = strip_pattern(&result, pattern);
    }

    // Clean up result
    clean_stripped_result(&result)
}

/// Strip only synth hardware from a string (for synth tracks specifically)
#[must_use]
pub fn strip_synth_hardware(input: &str) -> String {
    let mut result = input.to_string();

    for (pattern, _) in SYNTH_HARDWARE {
        result = strip_pattern(&result, pattern);
    }

    clean_stripped_result(&result)
}

/// Strip only mic/preamp models from a string
#[must_use]
pub fn strip_recording_equipment(input: &str) -> String {
    let mut result = input.to_string();

    for (pattern, _) in MIC_MODELS {
        result = strip_pattern(&result, pattern);
    }

    for (pattern, _) in PREAMP_MODELS {
        result = strip_pattern(&result, pattern);
    }

    clean_stripped_result(&result)
}

/// Strip only processing markers from a string
#[must_use]
pub fn strip_processing_markers(input: &str) -> String {
    let mut result = input.to_string();

    for pattern in PROCESSING_MARKERS {
        result = strip_pattern(&result, pattern);
    }

    clean_stripped_result(&result)
}

// --- Helper functions ---

/// Check if a word exists in the string with word boundaries
fn contains_word(haystack: &str, needle: &str) -> bool {
    let needle_lower = needle.to_lowercase();

    for (idx, _) in haystack.match_indices(&needle_lower) {
        let before_ok = idx.checked_sub(1).map_or(true, |idx_minus_1| {
            haystack
                .as_bytes()
                .get(idx_minus_1)
                .map_or(false, |b| !b.is_ascii_alphanumeric())
        });

        let after_idx = idx.checked_add(needle_lower.len()).unwrap_or(usize::MAX);
        let after_ok = if after_idx >= haystack.len() {
            true
        } else {
            haystack
                .as_bytes()
                .get(after_idx)
                .map_or(false, |b| !b.is_ascii_alphanumeric())
        };

        if before_ok && after_ok {
            return true;
        }
    }

    false
}

/// Strip a pattern from a string, respecting word boundaries
fn strip_pattern(input: &str, pattern: &str) -> String {
    let lower = input.to_lowercase();
    let pattern_lower = pattern.to_lowercase();

    // Find all occurrences with word boundaries
    let mut result = String::new();
    let mut last_end = 0;

    for (idx, _) in lower.match_indices(&pattern_lower) {
        let before_ok = idx.checked_sub(1).map_or(true, |idx_minus_1| {
            lower
                .as_bytes()
                .get(idx_minus_1)
                .map_or(false, |b| !b.is_ascii_alphanumeric())
        });

        let after_idx = idx.checked_add(pattern_lower.len()).unwrap_or(usize::MAX);
        let after_ok = if after_idx >= lower.len() {
            true
        } else {
            lower
                .as_bytes()
                .get(after_idx)
                .map_or(false, |b| !b.is_ascii_alphanumeric())
        };

        if before_ok && after_ok {
            // Found a word boundary match - skip it
            if let Some(slice) = input.get(last_end..idx) {
                result.push_str(slice);
            }
            last_end = after_idx;
        }
    }

    if let Some(slice) = input.get(last_end..) {
        result.push_str(slice);
    }
    result
}

/// Clean up a string after pattern stripping
fn clean_stripped_result(input: &str) -> String {
    let mut result = input.to_string();

    // Remove multiple consecutive separators
    while result.contains("..") {
        result = result.replace("..", ".");
    }
    while result.contains("__") {
        result = result.replace("__", "_");
    }
    while result.contains("  ") {
        result = result.replace("  ", " ");
    }
    while result.contains("- -") {
        result = result.replace("- -", "-");
    }
    while result.contains("--") {
        result = result.replace("--", "-");
    }

    // Remove leading/trailing separators
    let result = result
        .trim()
        .trim_matches(|c: char| c == '.' || c == '_' || c == '-' || c == ' ')
        .to_string();

    // Remove trailing " -" or "- " patterns
    let result = result
        .trim_end_matches(" -")
        .trim_end_matches("- ")
        .trim_start_matches("- ")
        .trim_start_matches(" -")
        .to_string();

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_mic_models() {
        assert_eq!(strip_equipment("Kick 57"), "Kick");
        assert_eq!(strip_equipment("Snare SM57"), "Snare");
        assert_eq!(strip_equipment("OH 414"), "OH");
        assert_eq!(strip_equipment("Vocal U47"), "Vocal");
        assert_eq!(strip_equipment("Bass 421"), "Bass");
    }

    #[test]
    fn strip_preamp_models() {
        assert_eq!(strip_equipment("Kick OptoComp"), "Kick");
        assert_eq!(strip_equipment("Vocal LA2A"), "Vocal");
        assert_eq!(strip_equipment("OH L 260VU"), "OH L");
        assert_eq!(strip_equipment("Bass 1176"), "Bass");
    }

    #[test]
    fn strip_processing_markers() {
        assert_eq!(strip_equipment("Kick LIM"), "Kick");
        assert_eq!(strip_equipment("Vocal EDT"), "Vocal");
        assert_eq!(strip_equipment("Snare No Compression"), "Snare");
        assert_eq!(strip_equipment("Vocal Printed"), "Vocal");
        assert_eq!(strip_equipment("Bass Bounced"), "Bass");
    }

    #[test]
    fn strip_synth_hardware() {
        assert_eq!(strip_equipment("Synth CASIO"), "Synth");
        assert_eq!(strip_equipment("Synth - CASIO CTK-601"), "Synth");
        assert_eq!(strip_equipment("Pad Roland D-50"), "Pad");
        assert_eq!(strip_equipment("Lead Moog"), "Lead");
        assert_eq!(strip_equipment("Weird Synth Patch - FA06"), "");
    }

    #[test]
    fn complex_names() {
        assert_eq!(
            strip_equipment("Breathy Synth 1 - Casio CTK-601 Briteness"),
            "Breathy Synth 1"
        );
        assert_eq!(strip_equipment("Bass Amp 44A. 260VU"), "Bass Amp");
        assert_eq!(strip_equipment("OH L Neumann"), "OH L");
    }

    #[test]
    fn preserve_content() {
        // Don't strip words that are part of content
        assert_eq!(strip_equipment("Compressed Air SFX"), "Compressed Air SFX");
        // Keep the core content
        assert_eq!(strip_equipment("Lead Vocal"), "Lead Vocal");
        assert_eq!(strip_equipment("Acoustic Guitar"), "Acoustic Guitar");
        // "Raw" and "Dry" are valid content descriptors
        assert_eq!(strip_equipment("Raw Guitar"), "Raw Guitar");
        assert_eq!(strip_equipment("Dry Vocal"), "Dry Vocal");
    }

    #[test]
    fn word_boundary_matching() {
        // "57" should match as word, not inside "257"
        assert_eq!(strip_equipment("Track 57"), "Track");
        assert_eq!(strip_equipment("Track257"), "Track257"); // No match - not a word boundary
    }

    #[test]
    fn real_world_examples() {
        // From neil_young_heart_of_gold
        assert_eq!(strip_equipment("Kick OptoComp"), "Kick");
        assert_eq!(strip_equipment("Snare No Compression"), "Snare");
        assert_eq!(strip_equipment("OH L 260VU"), "OH L");
        assert_eq!(strip_equipment("Bass Amp 44A. 260VU"), "Bass Amp");

        // From tears_for_fears_shout
        assert_eq!(
            strip_equipment("Breathy Synth 1 - Casio CTK-601 Briteness"),
            "Breathy Synth 1"
        );
        assert_eq!(strip_equipment("Weird Synth Patch - CASIO Charang"), "");

        // From steve_maggiora_hey_lady
        assert_eq!(strip_equipment("Vocal 47X"), "Vocal");
        assert_eq!(strip_equipment("Gtr 67X"), "Gtr");
    }
}

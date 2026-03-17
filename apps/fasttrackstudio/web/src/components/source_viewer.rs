//! Source Code Viewer Component
//!
//! Displays keyflow source code with basic syntax highlighting.

use dioxus::prelude::*;

/// Source code viewer with syntax highlighting.
#[component]
pub fn SourceViewer(source: &'static str) -> Element {
    // Split into lines for display
    let lines: Vec<&str> = source.lines().collect();

    rsx! {
        div {
            class: "bg-gray-900 rounded-lg border border-gray-700 overflow-hidden",

            // Code block with line numbers
            pre {
                class: "p-4 overflow-x-auto text-sm",

                code {
                    class: "font-mono",

                    for (i, line) in lines.iter().enumerate() {
                        div {
                            key: "{i}",
                            class: "flex",

                            // Line number
                            span {
                                class: "select-none text-gray-600 w-8 text-right pr-4 flex-shrink-0",
                                "{i + 1}"
                            }

                            // Line content with highlighting
                            HighlightedLine { line: *line }
                        }
                    }
                }
            }
        }
    }
}

/// A single line with syntax highlighting applied.
#[component]
fn HighlightedLine(line: &'static str) -> Element {
    let highlighted = highlight_line(line);

    rsx! {
        span {
            class: "flex-1",
            dangerous_inner_html: "{highlighted}"
        }
    }
}

/// Apply basic syntax highlighting to a line.
fn highlight_line(line: &str) -> String {
    let trimmed = line.trim();

    // Empty lines
    if trimmed.is_empty() {
        return String::new();
    }

    // Title line (first non-empty, non-directive line)
    // Comments (lines starting with //)
    if trimmed.starts_with("//") {
        return format!(
            r#"<span class="text-gray-500">{}</span>"#,
            escape_html(line)
        );
    }

    // Directives (lines starting with /)
    if trimmed.starts_with('/') && !trimmed.starts_with("//") {
        return format!(
            r#"<span class="text-purple-400">{}</span>"#,
            escape_html(line)
        );
    }

    // Metadata line (tempo, time signature, key)
    if trimmed.contains("bpm") || trimmed.contains("/4") || trimmed.starts_with('#') {
        return highlight_metadata(line);
    }

    // Section markers (VS, CH, BR, IN, OU, etc.)
    if is_section_marker(trimmed) {
        return format!(
            r#"<span class="text-blue-400 font-semibold">{}</span>"#,
            escape_html(line)
        );
    }

    // Chord lines
    highlight_chords(line)
}

/// Check if a line is a section marker.
fn is_section_marker(line: &str) -> bool {
    let markers = [
        "VS", "CH", "BR", "IN", "OU", "PC", "TG", "INT", "VT", "INTRO", "VERSE", "CHORUS",
        "BRIDGE", "OUTRO", "PRE", "TAG",
    ];
    markers
        .iter()
        .any(|m| line == *m || line.starts_with(&format!("{m} ")))
}

/// Highlight metadata line (tempo, time signature, key).
fn highlight_metadata(line: &str) -> String {
    let mut result = String::new();
    let mut current_pos = 0;

    // Find and highlight tempo (e.g., "120bpm")
    if let Some(bpm_pos) = line.find("bpm") {
        // Find the start of the tempo number
        let mut start = bpm_pos;
        while start > 0
            && line
                .chars()
                .nth(start - 1)
                .is_some_and(|c| c.is_ascii_digit())
        {
            start -= 1;
        }
        if start < bpm_pos {
            result.push_str(&escape_html(&line[current_pos..start]));
            result.push_str(&format!(
                r#"<span class="text-yellow-400">{}</span>"#,
                escape_html(&line[start..bpm_pos + 3])
            ));
            current_pos = bpm_pos + 3;
        }
    }

    // Find and highlight time signature (e.g., "4/4")
    for ts in ["4/4", "3/4", "6/8", "2/4", "12/8"] {
        if let Some(ts_pos) = line[current_pos..].find(ts) {
            let abs_pos = current_pos + ts_pos;
            result.push_str(&escape_html(&line[current_pos..abs_pos]));
            result.push_str(&format!(r#"<span class="text-orange-400">{}</span>"#, ts));
            current_pos = abs_pos + ts.len();
            break;
        }
    }

    // Find and highlight key (e.g., "#C", "#Ab")
    if let Some(key_pos) = line[current_pos..].find('#') {
        let abs_pos = current_pos + key_pos;
        result.push_str(&escape_html(&line[current_pos..abs_pos]));

        // Find the end of the key
        let key_end = line[abs_pos + 1..]
            .find(|c: char| c.is_whitespace())
            .map(|p| abs_pos + 1 + p)
            .unwrap_or(line.len());

        result.push_str(&format!(
            r#"<span class="text-green-400">{}</span>"#,
            escape_html(&line[abs_pos..key_end])
        ));
        current_pos = key_end;
    }

    result.push_str(&escape_html(&line[current_pos..]));
    result
}

/// Highlight chord symbols in a line.
fn highlight_chords(line: &str) -> String {
    let mut result = String::new();

    for part in line.split_inclusive(|c: char| c.is_whitespace() || c == '|') {
        let trimmed = part.trim_end_matches(|c: char| c.is_whitespace() || c == '|');

        if trimmed.is_empty() {
            result.push_str(&escape_html(part));
            continue;
        }

        // Check for push prefix
        let (has_push, chord_part) = if trimmed.starts_with('\'') {
            (true, &trimmed[1..])
        } else {
            (false, trimmed)
        };

        // Check if it looks like a chord
        let is_chord = chord_part.starts_with(|c: char| c.is_ascii_uppercase())
            || chord_part == "."
            || chord_part.starts_with('r')  // rests
            || chord_part.starts_with('s'); // simile

        if is_chord {
            // Push prefix in pink
            if has_push {
                result.push_str(r#"<span class="text-pink-400">'</span>"#);
            }

            // Chord name
            if chord_part == "." {
                result.push_str(r#"<span class="text-gray-500">.</span>"#);
            } else if chord_part.starts_with('r') || chord_part.starts_with('s') {
                result.push_str(&format!(
                    r#"<span class="text-gray-400">{}</span>"#,
                    escape_html(chord_part)
                ));
            } else {
                result.push_str(&format!(
                    r#"<span class="text-cyan-400">{}</span>"#,
                    escape_html(chord_part)
                ));
            }

            // Trailing whitespace/delimiter
            let suffix = &part[if has_push {
                1 + chord_part.len()
            } else {
                chord_part.len()
            }..];
            result.push_str(&escape_html(suffix));
        } else if trimmed == "/" || trimmed == "//" {
            // Slash notation
            result.push_str(&format!(
                r#"<span class="text-gray-500">{}</span>"#,
                escape_html(part)
            ));
        } else {
            result.push_str(&escape_html(part));
        }
    }

    result
}

/// Escape HTML special characters.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

use crate::chart::Chart;
use crate::chord::ChordRhythm;

/// Format rhythm notation for display
/// Default = "/" (single slash), Slashes = "///", Explicit = "_2." etc.
fn format_rhythm(rhythm: &ChordRhythm) -> String {
    use crate::core::duration::NoteValue;

    match rhythm {
        ChordRhythm::Default => "/".to_string(),
        ChordRhythm::Slashes {
            count,
            dotted,
            tied,
        } => {
            let mut s = "/".repeat(*count as usize);
            if *dotted {
                s.push('.');
            }
            if *tied {
                s.push('~');
            }
            s
        }
        ChordRhythm::Explicit(nd) => {
            // Convert NoteValue to lily notation (1, 2, 4, 8, 16, 32)
            let lily_val = match nd.note_value {
                NoteValue::Whole => 1,
                NoteValue::Half => 2,
                NoteValue::Quarter => 4,
                NoteValue::Eighth => 8,
                NoteValue::Sixteenth => 16,
                NoteValue::ThirtySecond => 32,
                NoteValue::SixtyFourth => 64,
            };

            let mut s = format!("_{}", lily_val);
            if nd.dots > 0 {
                s.push_str(&".".repeat(nd.dots as usize));
            }
            if let Some(tuplet) = &nd.tuplet {
                // Triplets (3) display as 't', other tuplets as ':n'
                if tuplet.numerator == 3 {
                    s.push('t');
                } else {
                    s.push_str(&format!(":{}", tuplet.numerator));
                }
            }
            s
        }
    }
}

/// Format a chord symbol for display, converting "maj" to "M"
///
/// Examples:
/// - "2maj" -> "2M"
/// - "Cmaj7" -> "CM7"
/// - "Dm" -> "Dm" (unchanged)
pub fn format_chord(s: &str) -> String {
    // Convert "maj" to "M" in chord symbols (e.g., "2maj" -> "2M", "Cmaj7" -> "CM7")
    s.replace("maj", "M")
}

impl std::fmt::Display for Chart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::chord::PushPullBase;

        // Simple title
        if let Some(ref title) = self.metadata.title {
            writeln!(f, "{}", title)?;
        }

        // Default push command if set
        if let Some(ref default_push) = self.default_push_amount {
            write!(f, "/push = ")?;
            match &default_push.base {
                PushPullBase::Standard => {
                    // Standard push (8th note)
                    writeln!(f, "8")?;
                }
                PushPullBase::Triplet => {
                    writeln!(f, "triplet")?;
                }
                PushPullBase::Tuplet(n) => {
                    writeln!(f, "tuplet:{}", n)?;
                }
                PushPullBase::Duration {
                    duration,
                    dotted,
                    triplet,
                } => {
                    write!(f, "{}", duration.value())?;
                    if *dotted {
                        write!(f, ".")?;
                    }
                    if *triplet {
                        write!(f, "t")?;
                    }
                    writeln!(f)?;
                }
            }
        }

        // Sections - one line per section
        for section in &self.sections {
            // Section header
            write!(f, "{}: ", section.section.display_name())?;

            // Collect all rhythm elements (chords and rests) from all measures
            let mut all_elements = Vec::new();
            for measure in section.measures() {
                if measure.rhythm_elements.is_empty() {
                    continue;
                }

                // Collect rhythm element strings for this measure
                let mut element_strings = Vec::new();
                for elem in &measure.rhythm_elements {
                    match elem {
                        crate::chart::types::RhythmElement::Chord(chord) => {
                            let mut chord_text = String::new();

                            // Accent command
                            if chord.commands.iter().any(|c| c.is_accent()) {
                                chord_text.push('>');
                            }

                            // Push/pull notation
                            if let Some((is_push, amount)) = &chord.push_pull
                                && *is_push
                            {
                                // Check if this push matches the default
                                let matches_default = self
                                    .default_push_amount
                                    .as_ref()
                                    .map(|default| {
                                        // Compare the push amounts
                                        format!("{:?}", amount) == format!("{:?}", default)
                                    })
                                    .unwrap_or(false);

                                if matches_default {
                                    // Just use apostrophe
                                    chord_text.push('\'');
                                } else {
                                    // Use full notation with amount
                                    chord_text.push_str(&Chart::format_push_pull_amount(amount));
                                }
                            }

                            chord_text.push_str(&chord.full_symbol);

                            // Note: Bass note is already included in full_symbol (from Chord::to_string())
                            // so we don't need to add it again here

                            // Pull notation (always show amount for pulls)
                            if let Some((is_push, _amount)) = &chord.push_pull
                                && !*is_push
                            {
                                chord_text.push('\'');
                            }

                            // Add rhythm notation
                            // - Explicit durations attach directly: Ab9_8t
                            // - Slashes are space-separated: 'Eb ///
                            // - Default (1 beat) = single slash: /
                            // - Whole measure (4 slashes in 4/4) = omitted
                            let ts = measure.time_signature;
                            match &chord.rhythm {
                                ChordRhythm::Slashes { count, .. } if *count == ts.0 => {
                                    // Whole measure — omit slashes entirely,
                                    // chord name alone implies full measure
                                }
                                ChordRhythm::Default => {
                                    // Single beat — show as /
                                    chord_text.push_str(" /");
                                }
                                ChordRhythm::Slashes { .. } => {
                                    // Partial slashes — space then slashes
                                    chord_text.push(' ');
                                    chord_text.push_str(&format_rhythm(&chord.rhythm));
                                }
                                _ => {
                                    // Explicit duration — attach directly
                                    chord_text.push_str(&format_rhythm(&chord.rhythm));
                                }
                            }

                            element_strings.push(chord_text);
                        }
                        crate::chart::types::RhythmElement::Rest(rest) => {
                            // Show rest with its original token (e.g., "r8t", "r2")
                            element_strings.push(rest.original_token.clone());
                        }
                        crate::chart::types::RhythmElement::Space(space) => {
                            // Show space with its original token (e.g., "s1", "s4")
                            element_strings.push(space.original_token.clone());
                        }
                    }
                }

                if !element_strings.is_empty() {
                    all_elements.push(element_strings.join(" "));
                }
            }

            // Print all elements separated by |
            if !all_elements.is_empty() {
                writeln!(f, "{}", all_elements.join(" | "))?;
            } else {
                writeln!(f)?;
            }
        }

        Ok(())
    }
}

//! Example: Manual chord-to-syllable alignment with Vienna
//!
//! Users explicitly assign chords to syllables using `{Chord}syllable` syntax
//! This gives full control over where chords appear in the vocal line

use keyflow_proto::LyricChordParser;

fn main() {
    println!("=== Vienna: Manual Chord-Syllable Assignment ===\n");

    let parser = LyricChordParser::new();

    // Example 1: Simple line with chord assignments
    let verse1 = "{Gm}Slow {A#}down you {F}cra-zy {Gm}child";

    println!("📝 Input: {}", verse1);
    println!("   (Chords explicitly assigned to syllables)\n");

    match parser.parse(verse1) {
        Ok(line) => {
            println!("✓ Parsed successfully\n");
            show_alignment(&line);
        }
        Err(e) => {
            println!("✗ Parse error: {}\n", e);
        }
    }

    // Example 2: Full verse with all chord assignments
    let full_verse = "{Gm}Slow down, {A#}you crazy {F}child\n\
                      {Gm}You're so {A#}am-bi-tious {F}for a {Gm}ju-ve-nile";

    println!("\n--- Full Verse Example ---\n");
    println!("📝 Input:\n{}\n", full_verse);

    // Parse each line
    for (line_num, lyric_line) in full_verse.lines().enumerate() {
        if lyric_line.trim().is_empty() {
            continue;
        }
        match parser.parse(lyric_line) {
            Ok(line) => {
                println!("Line {}: {} syllables", line_num + 1, line.syllables.len());
                for syl in line.syllables.iter() {
                    if let Some(chord) = &syl.chord {
                        println!("  • '{}' → {}", syl.text, chord);
                    } else {
                        println!("  • '{}' (no chord)", syl.text);
                    }
                }
                println!();
            }
            Err(e) => {
                println!("Line {}: Error: {}\n", line_num + 1, e);
            }
        }
    }

    // Example 3: Show the format syntax
    println!("\n--- Format Guide ---\n");
    show_format_guide();
}

fn show_alignment(line: &keyflow_proto::LyricLine) {
    println!("Syllables with chord assignments:");
    for (i, syl) in line.syllables.iter().enumerate() {
        let chord_marker = if let Some(chord) = &syl.chord {
            format!(" ← {}", chord)
        } else {
            String::new()
        };

        let hyphen = if syl.hyphen_after { "-" } else { "" };

        println!(
            "  [{}] '{}{}'{} (word_initial: {})",
            i, syl.text, hyphen, chord_marker, syl.word_initial
        );
    }

    println!("\n📊 Statistics:");
    let with_chords = line.syllables.iter().filter(|s| s.chord.is_some()).count();
    println!("  • Total syllables: {}", line.syllables.len());
    println!("  • With chords: {}", with_chords);
    println!("  • Without chords: {}", line.syllables.len() - with_chords);

    println!("\n🎵 Full text: {}", line.full_text());
}

fn show_format_guide() {
    println!("Syntax: {{ChordSymbol}}syllable {{NextChord}}next-syllable\n");

    println!("Examples:");
    println!("  {{Gm}}Slow        → Gm chord on 'Slow'");
    println!("  {{A#}}down        → A# chord on 'down'");
    println!("  {{Dm7}}a-ma-zing  → Dm7 on first syllable of 'amazing'");
    println!("  {{Cmaj7}}A {{Dm}}ma {{G}}zing → Different chords per syllable\n");

    println!("Features:");
    println!("  ✓ One chord per syllable");
    println!("  ✓ Chords in any position (before word, mid-word, etc.)");
    println!("  ✓ Works with hyphenated syllables (a-ma-zing)");
    println!("  ✓ Supports all chord notations (Cmaj7, Bb7#9, etc.)");
    println!("  ✓ Syllables without chords are also tracked\n");

    println!("Use Cases:");
    println!("  🎹 Synced playback: Each syllable knows its chord");
    println!("  📟 Lead sheets: Chords positioned exactly where they change");
    println!("  🎵 MIDI gen: Note timing matches syllable boundaries");
    println!("  🎤 Karaoke: Highlight syllable + show its chord");
    println!("  📱 Mobile: Interactive lyrics with chord lookup");
}

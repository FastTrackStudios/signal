//! Complete Vienna example: Full workflow from .kf file to chord-syllable alignments
//!
//! Demonstrates:
//! 1. Multi-block .kf document parsing
//! 2. Keyflow chord parsing
//! 3. Lyric parsing with manual chord assignments {Chord}syllable
//! 4. Chord-to-syllable alignment
//! 5. Use cases: synced lyrics, MIDI, interactive charts

use keyflow_proto::{ChordSyllableAligner, KfBlockKind, KfDocument, LyricChordParser};

fn main() {
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║           Vienna: Complete Chord-Syllable Workflow        ║");
    println!("╚════════════════════════════════════════════════════════════╝\n");

    // Step 1: Parse multi-block .kf document
    println!("📄 Step 1: Parse Multi-Block Document\n");

    let kf_content = r#"--- keyflow ---
Vienna (Live) - The Sweater Sessions
Billy Joel, Couch
120bpm 4/4 #Gm

vs verse 1
Gm //// A#//// F //// Gm////
[lyrics] {Gm}Slow down you {A#}crazy child, {F}you're so {Gm}am-bi-tious for a ju-ve-nile

--- chordpro ---
{title: Vienna (Live) - The Sweater Sessions}
{artist: Billy Joel, Couch}
{key: Gm}
{tempo: 120}

[Verse 1]
[Gm]Slow down, [A#]you crazy [F]child
You're so [Gm]ambitious for a juvenile
"#;

    let doc = parse_kf_document(kf_content).unwrap();
    println!("✓ Document parsed: {} blocks\n", doc.blocks.len());

    for (i, block) in doc.blocks.iter().enumerate() {
        println!("  Block {}: {} ({:?})", i + 1, block.name, block.kind);
    }

    // Step 2: Extract keyflow block and parse lyrics
    println!("\n📝 Step 2: Parse Lyrics with Chord Assignments\n");

    let _keyflow_block = doc.find_block("keyflow").unwrap();
    let lyrics_text =
        "{Gm}Slow down you {A#}crazy child, {F}you're so {Gm}am-bi-tious for a ju-ve-nile";

    let chord_parser = LyricChordParser::new();
    match chord_parser.parse(lyrics_text) {
        Ok(lyric_line) => {
            println!("✓ Parsed {} syllables:\n", lyric_line.syllables.len());

            for (i, syl) in lyric_line.syllables.iter().enumerate() {
                let chord_marker = syl
                    .chord
                    .as_ref()
                    .map(|c| format!(" → {}", c))
                    .unwrap_or_default();

                println!("  [{}] '{}'{}", i, syl.text, chord_marker);
            }

            // Step 3: Create mock chords for alignment demo
            println!("\n🎵 Step 3: Create Chord Instances\n");

            let chords = vec![
                create_mock_chord("Gm", 0, 0),
                create_mock_chord("A#", 0, 2),
                create_mock_chord("F", 0, 4),
                create_mock_chord("Gm", 0, 6),
            ];

            println!("✓ Created {} chords:\n", chords.len());
            for (i, chord) in chords.iter().enumerate() {
                println!(
                    "  [{}] {} at beat {}",
                    i, chord.full_symbol, chord.position.total_duration.beat
                );
            }

            // Step 4: Align chords to syllables
            println!("\n🔗 Step 4: Align Chords to Syllables\n");

            match ChordSyllableAligner::align(&chords, &lyric_line.syllables) {
                Ok(alignment) => {
                    println!(
                        "✓ Alignment successful: {} mappings\n",
                        alignment.mappings.len()
                    );

                    for mapping in &alignment.mappings {
                        let syl = &lyric_line.syllables[mapping.syllable_index];
                        let chord = &chords[mapping.chord_index];

                        println!(
                            "  Syllable '{}' ← Chord '{}' ({:?})",
                            syl.text, chord.full_symbol, mapping.attachment
                        );
                    }

                    // Step 5: Show use cases
                    show_use_cases(&alignment, &chords, &lyric_line.syllables);
                }
                Err(e) => println!("✗ Alignment error: {}", e),
            }
        }
        Err(e) => {
            println!("✗ Parse error: {}", e);
        }
    }

    // Step 6: Show ChordPro block
    println!("\n\n📋 Step 6: Alternative Format - ChordPro Block\n");

    if let Some(chordpro_block) = doc.find_block("chordpro") {
        println!("ChordPro format:\n{}", chordpro_block.content);
    }

    println!("\n╔════════════════════════════════════════════════════════════╗");
    println!("║                    Summary & Benefits                      ║");
    println!("╚════════════════════════════════════════════════════════════╝\n");

    println!("✨ What You Can Now Do:\n");
    println!("  1️⃣  Parse .kf files with multiple format blocks");
    println!("  2️⃣  Assign chords manually to syllables: {{Chord}}syllable");
    println!("  3️⃣  Generate chord-to-syllable mappings automatically");
    println!("  4️⃣  Enable synced playback with syllable-level timing");
    println!("  5️⃣  Create interactive chord charts with lyric lookups");
    println!("  6️⃣  Generate MIDI with exact syllable timing");
    println!("  7️⃣  Export to ChordPro or other formats\n");
}

fn parse_kf_document(content: &str) -> Result<KfDocument, String> {
    let mut blocks = Vec::new();
    let mut current_block_name = String::from("keyflow");
    let mut current_block_content = String::new();
    let mut found_delimiter = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("---") && trimmed.ends_with("---") && trimmed.len() > 6 {
            let middle = trimmed[3..trimmed.len() - 3].trim();
            if !middle.is_empty() {
                found_delimiter = true;

                if !current_block_content.trim().is_empty() {
                    let kind = KfBlockKind::from_name(&current_block_name);
                    blocks.push(keyflow_proto::KfBlock::new(
                        current_block_name.clone(),
                        kind,
                        current_block_content.trim().to_string(),
                    ));
                }

                current_block_name = middle.to_string();
                current_block_content = String::new();
                continue;
            }
        }

        if !current_block_content.is_empty() {
            current_block_content.push('\n');
        }
        current_block_content.push_str(line);
    }

    if !current_block_content.trim().is_empty() {
        let kind = KfBlockKind::from_name(&current_block_name);
        blocks.push(keyflow_proto::KfBlock::new(
            current_block_name,
            kind,
            current_block_content.trim().to_string(),
        ));
    }

    if !found_delimiter {
        blocks.push(keyflow_proto::KfBlock::keyflow(content));
    }

    Ok(keyflow_proto::KfDocument { blocks })
}

fn create_mock_chord(symbol: &str, measure: i32, beat: i32) -> keyflow_proto::ChordInstance {
    use keyflow_proto::{
        AbsolutePosition, MusicalDuration,
        chord::{ChordQuality, ChordRhythm},
        primitives::{MusicalNote, RootNotation},
    };

    // Extract root letter from symbol (e.g., "Gm" → "G", "A#" → "A#", "F" → "F")
    let root_str = symbol
        .chars()
        .take_while(|c| c.is_ascii_uppercase() || *c == '#' || *c == 'b')
        .collect::<String>();
    let root_note = MusicalNote::from_string(&root_str).unwrap_or_else(|| MusicalNote::c());
    let root = RootNotation::from_note_name(root_note.clone());

    let parsed =
        keyflow_proto::Chord::new(RootNotation::from_note_name(root_note), ChordQuality::Major);

    keyflow_proto::ChordInstance {
        root,
        full_symbol: symbol.to_string(),
        parsed,
        rhythm: ChordRhythm::Slashes {
            count: 2,
            dotted: false,
            tied: false,
        },
        original_token: symbol.to_string(),
        duration: MusicalDuration::new(0, 2, 0),
        position: AbsolutePosition::new(MusicalDuration::new(measure, beat, 0), 0),
        push_pull: None,
        commands: vec![],
        source_span: None,
        display_override: None,
    }
}

fn show_use_cases(
    alignment: &keyflow_proto::SectionAlignment,
    chords: &[keyflow_proto::ChordInstance],
    syllables: &[keyflow_proto::LyricSyllable],
) {
    println!("\n📱 Use Cases Enabled:\n");

    println!("🎹 Synced Karaoke:");
    for mapping in &alignment.mappings {
        let syl = &syllables[mapping.syllable_index];
        let chord = &chords[mapping.chord_index];
        println!(
            "   t={:.1}s: '{}' [{}] → {:.1}s duration",
            (mapping.chord_position.total_duration.beat as f32) * 0.5,
            syl.text,
            chord.full_symbol,
            (mapping.duration_until_next_chord.beat as f32) * 0.5
        );
    }

    println!("\n🎵 MIDI Note Timing:");
    println!("   Each syllable boundary triggers note changes");
    println!("   Exact timing = chord position + duration");

    println!("\n📟 Interactive Lead Sheet:");
    println!("   Click syllable → highlight its chord");
    println!("   Hover chord → highlight all its syllables");
    println!("   Bidirectional lookup enabled by alignment");
}

//! Example: Parsing Vienna (Billy Joel) lyrics with syllable-aware chord alignment
//!
//! This demonstrates how the SyllableParser handles real-world lyrics
//! with chord attachment at various positions.

use keyflow_proto::{ChordAttachment, SyllableParser};

fn main() {
    let parser = SyllableParser::new();

    // Example 1: Simple verse with inline chords
    let verse1 = "[Cmaj7] Slow down, [Dm7]you crazy [Cmaj7]child
You're so [Am7]ambitious for [Dm7]a juvenile";

    println!("=== Vienna - Verse 1 ===");
    let line1 = parser.parse(verse1);
    print_lyric_analysis(&line1);

    // Example 2: Chorus with word-by-word chords
    let chorus = "[Cmaj7]You know that [Dm7]when the truth is [G7]told
[Cmaj7]You can get [Am7]what you want or [Dm7]you can just [G7]get old";

    println!("\n=== Vienna - Chorus ===");
    let line2 = parser.parse(chorus);
    print_lyric_analysis(&line2);

    // Example 3: Explicit syllable marking (useful for precise timing)
    let explicit = "[Cmaj7]Slow|down you [Dm7]cra|zy child
[Am7]You're so [Dm7]am|bi|tious for a [G7]ju|ve|nile";

    println!("\n=== Vienna - With Explicit Syllable Marks ===");
    let line3 = parser.parse(explicit);
    print_lyric_analysis(&line3);

    // Show chord attachment positions
    println!("\n=== Chord Attachment Examples ===");
    show_attachment_examples();
}

fn print_lyric_analysis(line: &keyflow_proto::LyricLine) {
    for (idx, syl) in line.syllables.iter().enumerate() {
        let chord_info = if let Some(chord) = &syl.chord {
            format!(
                " [chord: {} @ {:?}]",
                chord,
                syl.chord_attachment.unwrap_or(ChordAttachment::BeforeWord)
            )
        } else {
            String::new()
        };

        let marker = if syl.word_initial { "»" } else { " " };

        println!(
            "  {} #{} '{}'{} {}",
            marker,
            idx,
            syl.text,
            if syl.hyphen_after { "-" } else { "" },
            chord_info
        );
    }

    println!("\nFull text: {}", line.full_text());
}

fn show_attachment_examples() {
    println!("\nChord placement options:");
    println!("  1. BeforeWord:       [C]Word → chord before all syllables");
    println!("  2. AtSyllableStart:  syl[C]lable → chord at syllable start");
    println!("  3. AtSyllableEnd:    syl[C]- → chord at syllable end");
    println!("  4. BetweenSyllables: syl- [C] -lable → chord between syllables");
    println!("  5. AfterWord:        word [C] → chord after all syllables");
    println!("\nUseful for:");
    println!("  - Melisma: multi-note syllables (syl-la-ble with C G D)");
    println!("  - Fast passages: chords between syllables");
    println!("  - Upbeats: chords before word for pickup notes");
}

use dawfile_ableton::*;

fn main() {
    let path = "crates/dawfile-ableton/tests/fixtures/LucidDreaming.als";
    println!("Reading: {path}");
    let set = read_live_set(path).expect("failed to parse");
    println!(
        "Parsed: {} audio, {} midi tracks",
        set.audio_tracks.len(),
        set.midi_tracks.len()
    );

    // Write to XML (not gzipped) so we can inspect
    let xml = serialize_to_xml(&set).expect("failed to serialize");
    let out_xml = "/tmp/roundtrip_output.xml";
    std::fs::write(out_xml, &xml).unwrap();
    println!(
        "Wrote XML to {out_xml} ({} bytes, {} lines)",
        xml.len(),
        xml.lines().count()
    );

    // Also write the original decompressed XML for comparison
    let original_bytes = std::fs::read(path).unwrap();
    let original_xml = {
        use flate2::read::GzDecoder;
        use std::io::Read;
        let mut decoder = GzDecoder::new(&original_bytes[..]);
        let mut xml = String::new();
        decoder.read_to_string(&mut xml).unwrap();
        xml
    };
    let orig_xml = "/tmp/original_output.xml";
    std::fs::write(orig_xml, &original_xml).unwrap();
    println!(
        "Wrote original XML to {orig_xml} ({} bytes, {} lines)",
        original_xml.len(),
        original_xml.lines().count()
    );

    // Also write as .als (gzipped)
    let out_als = "/tmp/roundtrip_output.als";
    write_live_set(&set, out_als).expect("failed to write .als");
    let als_size = std::fs::metadata(out_als).unwrap().len();
    println!("Wrote .als to {out_als} ({als_size} bytes)");

    // Compare key element counts
    fn count_tag(xml: &str, tag: &str) -> usize {
        xml.matches(&format!("<{tag}")).count()
    }

    println!("\n--- Element comparison (original vs roundtrip) ---");
    for tag in &[
        "AudioTrack",
        "MidiTrack",
        "GroupTrack",
        "ReturnTrack",
        "AudioClip",
        "MidiClip",
        "ClipSlot",
        "Locator",
        "Scene",
        "Eq8",
        "Compressor2",
        "GlueCompressor",
        "AutoFilter",
        "PluginDevice",
        "MxDeviceAudioEffect",
        "WarpMarker",
        "MidiNoteEvent",
        "AutomationEnvelope",
        "FloatEvent",
        "Pointee",
        "AutomationTarget",
        "SampleRef",
        "FileRef",
    ] {
        let orig = count_tag(&original_xml, tag);
        let rt = count_tag(&xml, tag);
        let status = if orig == rt { "  OK" } else { " DIFF" };
        println!("{status}  {tag}: {orig} -> {rt}");
    }
}

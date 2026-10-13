//! The header probe, against files written byte by byte.
//!
//! #158 could not state the DrumGizmo kits' sample rate or bit depth
//! because no wiki page says, so #176 has to read them off the files.
//! The kits themselves are gigabytes and cannot be a fixture, so what
//! is tested here is the reader — against the two shapes that actually
//! turned up (32-bit IEEE float, many channels) and the two that break
//! a naive reader (a chunk before `fmt `, and `WAVE_FORMAT_EXTENSIBLE`
//! hiding the real format in its sub-format GUID).

use std::path::Path;

use expression_editor_corpus::wav::{Format, probe, probe_tree, read_channel, summarize};

/// A `fmt ` chunk, and whatever leading chunks a test wants in front of
/// it.
fn wav(
    leading: &[(&[u8; 4], &[u8])],
    tag: u16,
    channels: u16,
    rate: u32,
    bits: u16,
    ext: &[u8],
) -> Vec<u8> {
    let mut fmt = Vec::new();
    fmt.extend(tag.to_le_bytes());
    fmt.extend(channels.to_le_bytes());
    fmt.extend(rate.to_le_bytes());
    let block_align = channels * bits / 8;
    fmt.extend((rate * block_align as u32).to_le_bytes());
    fmt.extend(block_align.to_le_bytes());
    fmt.extend(bits.to_le_bytes());
    fmt.extend(ext);

    let mut body = Vec::new();
    for (id, data) in leading {
        body.extend(*id);
        body.extend((data.len() as u32).to_le_bytes());
        body.extend(*data);
        if data.len() % 2 == 1 {
            body.push(0);
        }
    }
    body.extend(b"fmt ");
    body.extend((fmt.len() as u32).to_le_bytes());
    body.extend(&fmt);
    body.extend(b"data");
    body.extend(0u32.to_le_bytes());

    let mut out = Vec::from(*b"RIFF");
    out.extend((4 + body.len() as u32).to_le_bytes());
    out.extend(b"WAVE");
    out.extend(body);
    out
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("write");
    path
}

fn tmp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("drum-corpus-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

#[test]
fn a_crocell_shaped_header_reads_as_fifteen_channel_float() {
    // What CrocellKit 1.1 actually is: 15 channels, 48 kHz, 32-bit
    // IEEE float, one interleaved file per hit holding the whole mic
    // array. Neither the rate nor the format is stated on its page.
    let dir = tmp("crocell");
    let path = write(&dir, "hit.wav", &wav(&[], 3, 15, 48_000, 32, &[]));
    let header = probe(&path).expect("probes");
    assert_eq!(header.channels, 15);
    assert_eq!(header.sample_rate, 48_000);
    assert_eq!(header.bits_per_sample, 32);
    assert_eq!(header.format, Format::Float);
}

#[test]
fn a_drskit_shaped_header_reads_at_a_different_rate() {
    // And DRSKit 2.1 is 13 channels at 44.1 kHz. The two recommended
    // kits are at *different* sample rates, which is the finding a
    // harness that assumes one rate per corpus would trip over on its
    // second kit.
    let dir = tmp("drs");
    let path = write(&dir, "hit.wav", &wav(&[], 3, 13, 44_100, 32, &[]));
    let header = probe(&path).expect("probes");
    assert_eq!((header.channels, header.sample_rate), (13, 44_100));
    assert_eq!(header.format, Format::Float);
}

#[test]
fn a_chunk_in_front_of_fmt_does_not_derail_the_reader() {
    // `fmt ` at byte 12 is a convention, not a rule. A file with a
    // JUNK or bext chunk first is legal, and a fixed offset reads
    // garbage out of it.
    let dir = tmp("junk");
    let path = write(
        &dir,
        "hit.wav",
        &wav(
            &[(b"JUNK", &[0u8; 30]), (b"bext", &[7u8; 15])],
            1,
            2,
            44_100,
            24,
            &[],
        ),
    );
    let header = probe(&path).expect("probes");
    assert_eq!(header.channels, 2);
    assert_eq!(header.bits_per_sample, 24);
    assert_eq!(header.format, Format::Pcm);
}

#[test]
fn an_extensible_header_reports_the_format_it_actually_carries() {
    // Tag 0xFFFE says nothing on its own; the real tag is the first
    // two bytes of the sub-format GUID at offset 24 of the chunk.
    let dir = tmp("ext");
    let mut ext = Vec::new();
    ext.extend(22u16.to_le_bytes()); // cbSize
    ext.extend(32u16.to_le_bytes()); // valid bits
    ext.extend(0u32.to_le_bytes()); // channel mask
    ext.extend(3u16.to_le_bytes()); // sub-format: IEEE float
    ext.extend([0u8; 14]);
    let path = write(&dir, "hit.wav", &wav(&[], 0xFFFE, 8, 96_000, 32, &ext));
    let header = probe(&path).expect("probes");
    assert_eq!(header.format, Format::Float);
    assert_eq!((header.channels, header.sample_rate), (8, 96_000));
}

#[test]
fn a_file_that_is_not_a_wave_is_an_error_rather_than_a_guess() {
    let dir = tmp("bogus");
    let path = write(&dir, "hit.wav", b"not a wave file at all");
    assert!(probe(&path).is_err());
}

#[test]
fn probing_a_tree_groups_identical_headers() {
    // The useful form of a kit probe: 758 identical lines say nothing,
    // one row saying "758 × 15ch/48000/float32" is the whole answer,
    // and a second row is a mixed-rate kit worth knowing about before
    // rendering.
    let dir = tmp("tree");
    std::fs::create_dir_all(dir.join("Snare/samples")).expect("mkdir");
    write(
        &dir,
        "Snare/samples/1.wav",
        &wav(&[], 3, 15, 48_000, 32, &[]),
    );
    write(
        &dir,
        "Snare/samples/2.wav",
        &wav(&[], 3, 15, 48_000, 32, &[]),
    );
    write(
        &dir,
        "Snare/samples/3.wav",
        &wav(&[], 3, 15, 44_100, 32, &[]),
    );
    write(&dir, "Snare/notes.txt", b"ignored");

    let probed = probe_tree(&dir).expect("probes");
    assert_eq!(probed.len(), 3, "only .wav files are probed");
    // Sorted, so two runs over the same kit can be diffed.
    assert!(probed.windows(2).all(|w| w[0].0 <= w[1].0));

    let summary = summarize(&probed);
    assert_eq!(summary.len(), 2);
    assert_eq!(summary[0].1, 2, "the majority header comes first");
    assert_eq!(summary[0].0.sample_rate, 48_000);
    assert_eq!(summary[1].0.sample_rate, 44_100);
}

#[test]
fn one_mic_can_be_read_out_of_an_interleaved_file() {
    // The kits are one file per hit holding the whole array, so
    // measuring a single mic means deinterleaving. Summing is offered
    // and is usually wrong: mixing fifteen mics manufactures the bleed
    // the corpus exists to measure honestly.
    let dir = tmp("interleaved");
    let path = dir.join("array.wav");
    let spec = hound::WavSpec {
        channels: 3,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(&path, spec).expect("create");
    for frame in 0..8 {
        for ch in 0..3 {
            w.write_sample((frame * 10 + ch) as f32).expect("write");
        }
    }
    w.finalize().expect("finalize");

    let (mid, rate) = read_channel(&path, Some(1)).expect("reads");
    assert_eq!(rate, 48_000.0);
    assert_eq!(mid.len(), 8);
    assert_eq!(mid[3], 31.0);

    let (summed, _) = read_channel(&path, None).expect("reads");
    assert_eq!(summed[3], (30.0 + 31.0 + 32.0) / 3.0);

    assert!(read_channel(&path, Some(3)).is_err(), "channel 3 of 3");
}

//! Integration test: load a real `.signalpack`, dispatch a note-on, render
//! a block of audio, and assert the output is non-silent.
//!
//! Skipped (with a printed reason) when the AudioHaven sample tree isn't
//! mounted, mirroring the `aiff_real.rs` skip pattern.

use signal_sampler::{SamplerBank, read_pack_header};
use std::path::Path;

const STYLUS_DEFAULT_PACK: &str = "/run/media/AudioHaven/Sampled/Drum Kits/Stylus RMX-fresh/Stylus RMX/Core Library/Default/_ Please Select a Directory/library.signalpack";

#[test]
fn load_stylus_pack_renders_audio() {
    let pack = Path::new(STYLUS_DEFAULT_PACK);
    if !pack.exists() {
        eprintln!("skipped — pack not present at {}", pack.display());
        return;
    }

    // Header read should be cheap and produce a usable spec.
    let header = read_pack_header(pack).expect("read pack header");
    assert!(
        header.sample_count > 0,
        "expected at least one audio entry in the pack"
    );
    assert!(header.size_bytes > 0);

    // Determine which MIDI key plays something. For a groove-style pack,
    // from_pack synthesises one zone per groove rooted at slice_base_note
    // (default 36 = C2).
    let trigger_note = header
        .spec
        .grooves
        .first()
        .map(|g| {
            if g.slice_base_note == 0 {
                36
            } else {
                g.slice_base_note
            }
        })
        .or_else(|| header.spec.zones.first().map(|z| z.root_key))
        .expect("pack has neither grooves nor zones");

    // Drive the bank directly — no cpal stream. 48 kHz / stereo / 1024-frame
    // block is enough to hit the start of the loop.
    let mut bank = SamplerBank::new(48_000);
    bank.load_pack("test", pack).expect("load_pack");
    bank.note_on("test", trigger_note, 100);

    let mut out = vec![0.0f32; 1024 * 2];
    bank.render(&mut out);

    let rms = (out.iter().map(|&s| s as f64 * s as f64).sum::<f64>() / out.len() as f64).sqrt();
    assert!(
        rms > 1e-5,
        "expected non-zero RMS after note_on; got {rms} (peak={})",
        out.iter().fold(0.0f32, |a, &b| a.max(b.abs()))
    );
}

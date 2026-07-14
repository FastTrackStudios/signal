//! Build a `.signalpack` from a raw sample extraction directory.
//!
//!   cargo run -p signal-sampler --release --example build_pack -- \
//!       "<samples_root>" "<output.signalpack>"
//!
//! `<samples_root>/library.styx` is embedded verbatim as the pack spec (this is
//! exactly what `PlayerPatch::from_pack` reads back). Every audio file directly
//! under `<samples_root>` is decoded to FLAC-i24 and stored in the pack body.
//! This is the missing wrapper around `signal_sampler::engine::cache::
//! create_signal_pack` (the CLI that built the existing packs is not in-tree).

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let samples_root = PathBuf::from(
        args.next().expect("usage: build_pack <samples_root> <output.signalpack>"),
    );
    let output = PathBuf::from(
        args.next().expect("usage: build_pack <samples_root> <output.signalpack>"),
    );
    let spec = samples_root.join("library.styx");
    assert!(spec.exists(), "no library.styx in {}", samples_root.display());

    // Collect the audio samples (flat, sorted for a deterministic pack layout).
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&samples_root)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| matches!(e.to_ascii_lowercase().as_str(), "flac" | "wav" | "aif" | "aiff"))
                .unwrap_or(false)
        })
        .collect();
    paths.sort();
    eprintln!(
        "build_pack: {} samples\n  from {}\n  ->   {}",
        paths.len(),
        samples_root.display(),
        output.display()
    );

    let t = std::time::Instant::now();
    let stats = signal_sampler::engine::cache::create_signal_pack(
        &output,
        &spec,
        &samples_root,
        paths.iter().map(PathBuf::as_path),
    )?;
    eprintln!("build_pack: done in {:?} — {stats:?}", t.elapsed());
    if stats.failed > 0 {
        // Some source files are undecodable (the original packs skipped these
        // too). Warn loudly but still produce the pack — a partial pack matches
        // the historical builds. Fail only if *nothing* packed.
        eprintln!("build_pack: WARNING — {} sample(s) skipped (undecodable)", stats.failed);
    }
    if stats.prepared == 0 {
        return Err("no samples packed".into());
    }
    Ok(())
}

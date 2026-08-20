//! `verify_pack_lossless` — prove a pack's audio matches the files it was
//! built from, sample for sample.
//!
//! ```bash
//! cargo run -p signal-sampler --release --example verify_pack_lossless -- \
//!     <pack> <samples_root> [entries]
//! ```
//!
//! `stream_verify` compares the pack against *itself* (streamed decode vs
//! whole decode), which catches chunk-offset bugs but says nothing about
//! whether the pack still holds the audio that went in. This compares the
//! decoded pack entry against the decoded source file on disk.
//!
//! It exists because a lossless codec plus a plausible compression ratio is an
//! *inference*, not a measurement — and a pack that quietly lost bit depth
//! would show neither a warning nor an obviously wrong size.
//!
//! For a lossless pack (`flac-i24`, `pcm-i16`, `pcm-i24`) the tolerance is a
//! true zero: every sample must be bit-identical after the shared normalise-to-
//! f32 read. A lossy pack (`ogg-vorbis`) is reported as peak/RMS error instead,
//! since exactness is not the contract there.

use std::path::{Path, PathBuf};

use signal_sampler::engine::cache::{load_sample, SampleCache, SignalPcmPack};

fn short(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn main() -> eyre::Result<()> {
    let mut args = std::env::args().skip(1);
    let pack_path = args.next().ok_or_else(|| {
        eyre::eyre!("usage: verify_pack_lossless <pack> <samples_root> [entries]")
    })?;
    let samples_root = PathBuf::from(args.next().ok_or_else(|| {
        eyre::eyre!("usage: verify_pack_lossless <pack> <samples_root> [entries]")
    })?);
    let count: usize = args.next().and_then(|c| c.parse().ok()).unwrap_or(8);

    // Whole-entry decode, so a streaming bug can't be mistaken for a fidelity
    // one — that is stream_verify's job, not this one.
    // SAFETY: set before any other thread reads it.
    unsafe { std::env::set_var("FTS_STREAM", "off") };

    let pack = SignalPcmPack::open(Path::new(&pack_path))?;
    let lossless = !pack.kind_label().contains("vorbis");
    println!(
        "pack {pack_path}\n  codec {} ({})",
        pack.kind_label(),
        if lossless {
            "expect bit-exact"
        } else {
            "lossy — reporting error magnitude"
        }
    );

    let mut entries: Vec<PathBuf> = pack.entries_iter().map(|(p, _)| p.clone()).collect();
    entries.sort();
    // Spread the sample across the pack rather than taking the first N, which
    // in a piano pack would all be the bottom octave of one articulation.
    let stride = (entries.len() / count.max(1)).max(1);
    let chosen: Vec<PathBuf> = entries
        .iter()
        .step_by(stride)
        .take(count)
        .cloned()
        .collect();

    let cache = SampleCache::with_pack(SignalPcmPack::open(Path::new(&pack_path))?);
    let (mut checked, mut failed, mut missing) = (0usize, 0usize, 0usize);
    let mut worst_overall = 0.0f32;

    for path in &chosen {
        let source = samples_root.join(path);
        if !source.exists() {
            println!("  {:<52} SOURCE MISSING", short(path));
            missing += 1;
            continue;
        }
        let packed = cache.get(path.as_path())?;
        let orig = load_sample(&source)?;

        if packed.channels != orig.channels || packed.sample_rate != orig.sample_rate {
            println!(
                "  {:<52} FORMAT DRIFT  pack {}ch/{}Hz vs source {}ch/{}Hz",
                short(path),
                packed.channels,
                packed.sample_rate,
                orig.channels,
                orig.sample_rate
            );
            failed += 1;
            continue;
        }
        if packed.num_frames != orig.num_frames {
            println!(
                "  {:<52} FRAME COUNT  pack {} vs source {}",
                short(path),
                packed.num_frames,
                orig.num_frames
            );
            failed += 1;
            continue;
        }

        let total = orig.num_frames * orig.channels.max(1) as usize;
        let (mut worst, mut worst_at) = (0.0f32, 0usize);
        for i in 0..total {
            let d = (packed.pcm.sample(i) - orig.pcm.sample(i)).abs();
            if d > worst {
                worst = d;
                worst_at = i;
            }
        }
        worst_overall = worst_overall.max(worst);
        checked += 1;

        let ok = if lossless { worst == 0.0 } else { worst < 0.35 };
        if ok {
            println!(
                "  {:<52} ok   {} frames, worst |Δ| {worst:.3e}",
                short(path),
                orig.num_frames
            );
        } else {
            failed += 1;
            println!(
                "  {:<52} FAIL worst |Δ| {worst:.3e} at sample {worst_at}",
                short(path)
            );
        }
    }

    println!("\n{checked} checked, {failed} failed, {missing} source-missing");
    println!("worst |Δ| across all: {worst_overall:.3e}");
    if failed > 0 || checked == 0 {
        eyre::bail!("pack does not match its sources");
    }
    println!(
        "{}",
        if lossless {
            "PASS — bit-exact against source"
        } else {
            "PASS — within lossy tolerance"
        }
    );
    Ok(())
}

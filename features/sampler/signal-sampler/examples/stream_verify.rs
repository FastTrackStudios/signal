//! `stream_verify` — decode a pack entry both ways and compare, sample by
//! sample.
//!
//! The streamed path reads a head plus chunks decoded from frame offsets; the
//! reference path decodes the whole entry. If a chunk lands at the wrong
//! offset, or a decode comes up short, the audio doesn't go quiet — it goes
//! *wrong*, which is what static in the sustain sounds like. This finds the
//! first sample where the two disagree and says which chunk it was in.
//!
//! ```bash
//! cargo run -p signal-sampler --release --example stream_verify -- <pack> [entries]
//! ```

use std::path::Path;

use signal_sampler::engine::cache::{SampleCache, SignalPcmPack};
use signal_sampler::engine::stream::{StreamedSample, CHUNK_FRAMES, HEAD_FRAMES};

fn main() -> eyre::Result<()> {
    let pack_path = std::env::args()
        .nth(1)
        .ok_or_else(|| eyre::eyre!("usage: stream_verify <pack> [entries]"))?;
    let count: usize = std::env::args()
        .nth(2)
        .and_then(|c| c.parse().ok())
        .unwrap_or(4);

    let pack = SignalPcmPack::open(Path::new(&pack_path))?;
    println!("pack {} ({})", pack_path, pack.kind_label());

    let mut entries: Vec<_> = pack
        .entries_iter()
        .map(|(p, e)| (p.clone(), e.clone()))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries.truncate(count);

    // The reference: whole-entry decode, through the ordinary cache with
    // streaming turned off for this process.
    // SAFETY: set before any other thread reads it.
    unsafe { std::env::set_var("FTS_STREAM", "off") };
    let cache = SampleCache::with_pack(SignalPcmPack::open(Path::new(&pack_path))?);

    let mut failed = 0usize;
    for (path, entry) in &entries {
        let reference = cache.get(path)?;
        let map = pack.mmap_handle();
        let Some(streamed) = StreamedSample::open(
            map.clone(),
            entry.offset() as usize,
            entry.bytes() as usize,
            entry.channels(),
            entry.sample_rate(),
            entry.num_frames(),
        ) else {
            println!(
                "  {:<50} NOT INDEXABLE (falls back to whole decode)",
                short(path)
            );
            continue;
        };

        // Walk the whole sample, waiting for chunks the way the streamer's
        // consumers do. Compare against the reference decode.
        let total = entry.num_frames() * entry.channels().max(1) as usize;
        let (mut worst, mut worst_at, mut misses) = (0.0f32, 0usize, 0usize);
        for i in 0..total {
            let want = reference.pcm.sample(i);
            let mut got = streamed.sample(i);
            if got == 0.0 && want.abs() > 1e-4 {
                misses += 1;
                for _ in 0..100 {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                    got = streamed.sample(i);
                    if got != 0.0 {
                        break;
                    }
                }
            }
            let err = (got - want).abs();
            if err > worst {
                worst = err;
                worst_at = i;
            }
        }
        // i16 quantisation is the floor: one LSB is 1/32768.
        let ok = worst <= 4.0 / 32768.0;
        if !ok {
            failed += 1;
        }
        let ch = entry.channels().max(1) as usize;
        println!(
            "  {:<50} {} worst {:.5} at sample {} (frame {}, {}) · {} first-read misses",
            short(path),
            if ok { "ok  " } else { "BAD " },
            worst,
            worst_at,
            worst_at / ch,
            region(worst_at / ch),
            misses,
        );
    }
    if failed == 0 {
        println!("\nPASS — streamed audio matches the decode within i16 quantisation");
        Ok(())
    } else {
        println!("\nFAIL — {failed} of {} entries diverge", entries.len());
        std::process::exit(1);
    }
}

fn region(frame: usize) -> String {
    if frame < HEAD_FRAMES as usize {
        "head".into()
    } else {
        format!("chunk {}", frame as u32 / CHUNK_FRAMES)
    }
}

fn short(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

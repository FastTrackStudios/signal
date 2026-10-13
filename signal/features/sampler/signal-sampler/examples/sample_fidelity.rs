//! Does the PLAYBACK path reproduce the sample?
//!
//! Every metric this session measures timing: was the callback late, did a
//! chunk arrive. All of them read clean while the rig still crackles, and
//! forcing a 4x bigger block changed nothing — so the remaining suspect is
//! the signal path itself, where timing does not exist.
//!
//! This renders a real pack entry through a real [`Voice`] at unity rate
//! (no pitch shift, no filter, no envelope) and compares it sample for
//! sample against `decode_all` — the same bytes, decoded in bulk with no
//! streaming, no cursor and no interpolation. Any difference is ours.
//!
//! ```bash
//! cargo run --release -p signal-sampler --example sample_fidelity -- <pack> [entry-index]
//! ```
use std::path::Path;
use std::sync::Arc;

use signal_sampler::engine::cache::{SampleData, SignalPcmPack};
use signal_sampler::engine::stream::StreamedSample;
use signal_sampler::engine::voice::{Voice, VoiceKind};

fn main() -> eyre::Result<()> {
    let pack_path = std::env::args()
        .nth(1)
        .ok_or_else(|| eyre::eyre!("usage: sample_fidelity <pack> [entry-index]"))?;
    let want: usize = std::env::args()
        .nth(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let pack = SignalPcmPack::open(Path::new(&pack_path))?;
    let mut entries: Vec<_> = pack
        .entries_iter()
        .map(|(p, e)| (p.clone(), e.clone()))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let (path, entry) = entries
        .into_iter()
        .nth(want)
        .ok_or_else(|| eyre::eyre!("no entry {want}"))?;
    println!(
        "{} — {} frames, {} ch, {} Hz",
        path.display(),
        entry.num_frames(),
        entry.channels(),
        entry.sample_rate()
    );

    let stream = StreamedSample::open(
        pack.mmap_handle(),
        entry.offset() as usize,
        entry.bytes() as usize,
        entry.channels(),
        entry.sample_rate(),
        entry.num_frames(),
    )
    .ok_or_else(|| eyre::eyre!("entry is not indexable"))?;

    // Ground truth: the whole entry decoded in bulk, no streaming involved.
    let truth = stream.decode_all();
    let ch = entry.channels().max(1) as usize;

    // The playback path: a voice over the STREAMED sample, unity rate.
    let data = Arc::new(SampleData::streamed(Arc::clone(&stream)));
    let mut v = Voice::new(data, 60, VoiceKind::Zoned, 0, 1.0, 48_000);

    // Render at realtime pace so the streamer is neither starved nor given
    // an unfair head start — the condition a player actually creates.
    let frames = entry.num_frames().min(48_000 * 8);
    let mut out = vec![0.0f32; frames * 2];
    let block = 512usize;
    let mut done = 0usize;
    while done < frames {
        let n = block.min(frames - done);
        v.render_block(&mut out[done * 2..(done + n) * 2]);
        done += n;
        std::thread::sleep(std::time::Duration::from_micros(10_667));
    }

    // Compare. Only the LEFT channel: the voice pans, and a pan law is not
    // a fidelity bug.
    let (mut worst, mut worst_at, mut bad) = (0.0f32, 0usize, 0usize);
    let mut silent_where_signal = 0usize;
    for f in 0..frames {
        let want = truth.get(f * ch).copied().unwrap_or(0.0);
        let got = out[f * 2];
        let d = (got - want).abs();
        if d > worst {
            worst = d;
            worst_at = f;
        }
        if d > 0.01 {
            bad += 1;
        }
        if got == 0.0 && want.abs() > 0.01 {
            silent_where_signal += 1;
        }
    }
    println!(
        "compared {frames} frames: worst |diff| = {worst:.6} at frame {worst_at} \
         ({:.3}s) | frames off by >0.01: {bad} ({:.3}%) | silent-where-signal: {silent_where_signal}",
        worst_at as f64 / f64::from(entry.sample_rate()),
        100.0 * bad as f64 / frames as f64,
    );
    if bad == 0 && silent_where_signal == 0 {
        println!("PASS: playback reproduces the sample");
    } else {
        println!("FAIL: the playback path is not reproducing the sample");
    }
    Ok(())
}

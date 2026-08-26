//! `stream_debug` — watch one streamed sample deliver (or fail to deliver)
//! its chunks, at realtime pace.

use std::path::Path;

use signal_sampler::engine::cache::SignalPcmPack;
use signal_sampler::engine::stream::StreamedSample;

fn main() -> eyre::Result<()> {
    let pack_path = std::env::args().nth(1).expect("usage: stream_debug <pack>");
    let pack = SignalPcmPack::open(Path::new(&pack_path))?;
    let mut entries: Vec<_> = pack
        .entries_iter()
        .map(|(p, e)| (p.clone(), e.clone()))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let (path, entry) = entries.into_iter().next().expect("an entry");
    println!(
        "{} — {} frames, {} ch, {} Hz, {} bytes",
        path.display(),
        entry.num_frames(),
        entry.channels(),
        entry.sample_rate(),
        entry.bytes(),
    );

    let s = StreamedSample::open(
        pack.mmap_handle(),
        entry.offset() as usize,
        entry.bytes() as usize,
        entry.channels(),
        entry.sample_rate(),
        entry.num_frames(),
    )
    .expect("indexable");
    println!("head resident: {} bytes", s.resident_bytes());

    // Walk at realtime pace, 512-frame blocks, reporting what each block saw.
    let ch = entry.channels().max(1) as usize;
    let mut frame = 0usize;
    for block in 0..120 {
        let mut nonzero = 0;
        for _ in 0..512 {
            if frame * ch < entry.num_frames() * ch && s.sample(frame * ch) != 0.0 {
                nonzero += 1;
            }
            frame += 1;
        }
        if block % 4 == 0 {
            println!(
                "  t={:>5.2}s frame {:>7}  nonzero {:>3}/512  resident {:>8} B",
                block as f64 * 512.0 / entry.sample_rate() as f64,
                frame,
                nonzero,
                s.resident_bytes(),
            );
        }
        std::thread::sleep(std::time::Duration::from_micros(10_667));
    }
    Ok(())
}

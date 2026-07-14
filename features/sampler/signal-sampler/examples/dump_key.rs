//! Dump the full parsed SampleKey for stems on stdin (one per line):
//!   articulation \t mic \t dynamic \t note \t direction \t rr
//! Reveals how the runtime maps Keyscape filenames (mic collisions, wrong root
//! notes, etc.).  … | cargo run -p signal-sampler --release --example dump_key

use std::io::{BufRead, Write};

use signal_sampler::sample_map::parse_sample_stem;

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    for line in stdin.lock().lines() {
        let line = line.unwrap_or_default();
        let stem = line.trim_end();
        if stem.is_empty() {
            continue;
        }
        match parse_sample_stem(stem) {
            Some(k) => {
                let _ = writeln!(
                    out,
                    "{stem}\t=> art={} mic={:?} dyn={:?} note={} dir={:?} rr={}",
                    k.articulation, k.mic, k.dynamic, k.note, k.direction, k.rr
                );
            }
            None => {
                let _ = writeln!(out, "{stem}\t=> <unparsed>");
            }
        }
    }
}

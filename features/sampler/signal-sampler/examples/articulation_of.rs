//! Map sample stems to their runtime articulation id (the same id the engine and
//! styx use). Reads one stem (filename without extension) per line on stdin,
//! prints the parsed articulation per line ("" if the stem doesn't parse).
//! Used by the release-kind fixer to map .db samples -> styx articulation ids
//! without re-guessing the (scheme-specific) id derivation.
//!   … | cargo run -p signal-sampler --release --example articulation_of

use std::io::{BufRead, Write};

use signal_sampler::sample_map::parse_sample_stem;

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    for line in stdin.lock().lines() {
        let line = line.unwrap_or_default();
        let stem = line.trim_end();
        let art = parse_sample_stem(stem)
            .map(|k| k.articulation)
            .unwrap_or_default();
        let _ = writeln!(out, "{art}");
    }
}

//! List all fades on each track using the parser.

use std::env;

fn main() {
    let path = env::args().nth(1).expect("usage: list_fades <file>");
    let session = dawfile_protools::read_session(&path, 0).expect("parse");
    println!(
        "session: {} tracks  sr={}",
        session.audio_tracks.len(),
        session.session_sample_rate
    );
    let sr = session.session_sample_rate as f64;
    for t in &session.audio_tracks {
        if t.fades.is_empty() {
            continue;
        }
        println!("\nTrack: {}", t.name);
        for f in &t.fades {
            println!(
                "  start={:.3}s  in={:.3}s  out={:.3}s  shape={}  fade_idx={}",
                f.start_pos as f64 / sr,
                f.in_length as f64 / sr,
                f.out_length as f64 / sr,
                f.shape,
                f.fade_index,
            );
        }
    }
}

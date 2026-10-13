fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        "crates/dawfile-protools/tests/fixtures/studio-session-2.ptx".to_string()
    });
    let session = dawfile_protools::read_session(&path, 48000).unwrap();
    let rate = session.session_sample_rate;

    for track in &session.audio_tracks {
        if track.alternate_playlists.is_empty() {
            continue;
        }
        println!("track={:?} playlist={:?}", track.name, track.playlist_name);
        for r in &track.regions {
            println!(
                "  [active] region={} start={:.3}s",
                r.region_index,
                r.start_pos as f64 / rate as f64
            );
        }
        for alt in &track.alternate_playlists {
            println!("  [alt {:?}]", alt.name);
            for r in &alt.regions {
                println!(
                    "    region={} start={:.3}s",
                    r.region_index,
                    r.start_pos as f64 / rate as f64
                );
            }
        }
    }
}

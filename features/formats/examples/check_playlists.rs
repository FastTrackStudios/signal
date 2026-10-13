fn dump_session(path: &str, label: &str) {
    let session = match formats::protools::read_session(path, 48000) {
        Ok(s) => s,
        Err(e) => {
            println!("=== {label} ===\n  ERROR: {e:?}");
            return;
        }
    };
    println!("=== {label} ===");
    println!(
        "  version={} rate={}",
        session.version, session.session_sample_rate
    );
    println!(
        "  audio_files={} audio_regions={} audio_tracks={}",
        session.audio_files.len(),
        session.audio_regions.len(),
        session.audio_tracks.len()
    );
    for t in &session.audio_tracks {
        println!(
            "  track={:?} playlist={:?} regions={} alternates={}",
            t.name,
            t.playlist_name,
            t.regions.len(),
            t.alternate_playlists.len()
        );
        for alt in &t.alternate_playlists {
            println!(
                "    alt playlist={:?} regions={}",
                alt.name,
                alt.regions.len()
            );
        }
    }
}

fn main() {
    let fixtures = [
        "features/formats/tests/fixtures/protools/wonder-session.ptx",
        "features/formats/tests/fixtures/protools/studio-session-2.ptx",
        "features/formats/tests/fixtures/protools/studio-comp-session.ptx",
        "features/formats/tests/fixtures/protools/orchestral-session.ptx",
        "features/formats/tests/fixtures/protools/choir-session.ptx",
        "features/formats/tests/fixtures/protools/studio-tracking-session.ptx",
        "features/formats/tests/fixtures/protools/worship-session.ptx",
        "features/formats/tests/fixtures/protools/live-concert-session.ptx",
        "features/formats/tests/fixtures/protools/GodnessOfGod.ptx",
        "features/formats/tests/fixtures/protools/goodplaylists2.ptf",
    ];
    for path in &fixtures {
        let label = std::path::Path::new(path)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        dump_session(path, label);
        println!();
    }
}

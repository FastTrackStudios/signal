fn dump_session(path: &str, label: &str) {
    let session = match dawfile_protools::read_session(path, 48000) {
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
        "crates/dawfile-protools/tests/fixtures/wonder-session.ptx",
        "crates/dawfile-protools/tests/fixtures/studio-session-2.ptx",
        "crates/dawfile-protools/tests/fixtures/studio-comp-session.ptx",
        "crates/dawfile-protools/tests/fixtures/orchestral-session.ptx",
        "crates/dawfile-protools/tests/fixtures/choir-session.ptx",
        "crates/dawfile-protools/tests/fixtures/studio-tracking-session.ptx",
        "crates/dawfile-protools/tests/fixtures/worship-session.ptx",
        "crates/dawfile-protools/tests/fixtures/live-concert-session.ptx",
        "crates/dawfile-protools/tests/fixtures/GodnessOfGod.ptx",
        "crates/dawfile-protools/tests/fixtures/goodplaylists2.ptf",
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

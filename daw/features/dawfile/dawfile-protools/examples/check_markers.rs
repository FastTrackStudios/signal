fn main() {
    let fixtures = [
        "crates/dawfile-protools/tests/fixtures/studio-session-2.ptx",
        "crates/dawfile-protools/tests/fixtures/worship-session.ptx",
        "crates/dawfile-protools/tests/fixtures/wonder-session.ptx",
    ];
    for path in &fixtures {
        let s = dawfile_protools::read_session(path, 48000).unwrap();
        println!(
            "=== {} ===",
            std::path::Path::new(path)
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
        );
        println!(
            "  Markers ({}): {:?}",
            s.markers.len(),
            s.markers.iter().map(|m| &m.name).collect::<Vec<_>>()
        );
    }
}

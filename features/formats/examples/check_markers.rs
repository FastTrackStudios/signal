fn main() {
    let fixtures = [
        "features/formats/tests/fixtures/protools/studio-session-2.ptx",
        "features/formats/tests/fixtures/protools/worship-session.ptx",
        "features/formats/tests/fixtures/protools/wonder-session.ptx",
    ];
    for path in &fixtures {
        let s = formats::protools::read_session(path, 48000).unwrap();
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

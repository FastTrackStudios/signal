#[test]
fn parse_stylus_rmx_grooves() {
    use signal_sampler::LibrarySpec;
    let path = std::path::Path::new(
        "/run/media/AudioHaven/Sampled/Drum Kits/Stylus RMX-fresh/Stylus RMX/Core Library/RMX Grooves/library.styx",
    );
    if !path.exists() {
        return;
    }
    let spec = LibrarySpec::from_file(path).expect("parse Stylus RMX library.styx");
    assert!(
        spec.grooves.len() > 1000,
        "expected >1000 grooves, got {}",
        spec.grooves.len()
    );
    // Every groove should have a BPM in the realistic range.
    for g in &spec.grooves {
        assert!(
            g.bpm >= 30.0 && g.bpm <= 220.0,
            "{:?} bpm={}",
            g.label,
            g.bpm
        );
    }
    // At least one groove should carry slice markers.
    let with_slices = spec.grooves.iter().filter(|g| !g.slices.is_empty()).count();
    assert!(with_slices > 0, "no grooves with slices");
    // Standard slice-base note is C2 = 36.
    assert!(spec.grooves.iter().any(|g| g.slice_base_note == 36));
}

#[test]
fn parse_omnisphere_wavetable_styx() {
    use signal_sampler::LibrarySpec;
    let path = std::path::Path::new(
        "/run/media/AudioHaven/Sampled/Synth/Omnisphere-Wavetables/4 - EDM Wavetables/Spectroid/Xenomorphic/library.styx",
    );
    if !path.exists() {
        return;
    }
    let spec = LibrarySpec::from_file(path).expect("parse wavetable styx");
    assert_eq!(spec.name, "Xenomorphic");
    assert!(!spec.wavetables.is_empty(), "wavetables empty");
    let w0 = &spec.wavetables[0];
    assert_eq!(w0.frame_count, 128);
    assert_eq!(w0.cycle_length, 2048);
    assert!(w0.file.ends_with(".wav") || w0.file.ends_with(".stmwf"));
    assert_eq!(w0.category, "4 - EDM Wavetables");
}

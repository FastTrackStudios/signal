#[test]
fn parse_omnisphere_zonemap() {
    use signal_sampler::LibrarySpec;
    let path = std::path::Path::new("/tmp/zonemap-test/CS-80 PWM/library.styx");
    if !path.exists() {
        return;
    }
    let spec = LibrarySpec::from_file(path).expect("parse zonemap styx");
    assert_eq!(spec.name, "CS-80 PWM");
    assert_eq!(spec.zones.len(), 37);
    let z0 = &spec.zones[0];
    assert_eq!(z0.key_min, 0);
    assert_eq!(z0.key_max, 24);
    assert_eq!(z0.root_key, 24);
    assert!(z0.file.contains("CS-80 PWM.01.wav"));
}

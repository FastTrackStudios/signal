#[test]
fn parse_mm2_hi_hat_styx() {
    use signal_sampler::LibrarySpec;
    let path = std::path::Path::new(
        "/run/media/AudioHaven/Sampled/Drum Kits/MM2_extracted/resources/samples/Modern & Massive 2/07_Hats/01_15'' Zildjian A Custom Mastersound Hats/library.styx",
    );
    if !path.exists() {
        return;
    }
    let spec = LibrarySpec::from_file(path).expect("parse MM2 styx");
    assert!(spec.name.contains("Mastersound"));
    assert_eq!(spec.vendor, "GGD Modern and Massive 2");
    assert_eq!(spec.mics.len(), 5);
    assert_eq!(spec.zones.len(), 3130);
    // Hi-hat has Closed (42) + Open (46) + Pedal (44) zone groups;
    // articulations are subtypes ("Closed Tip" / "Open 1" / "Pedal Chick").
    assert!(
        spec.zones
            .iter()
            .any(|z| z.key_min == 42 && z.articulation.starts_with("Closed"))
    );
    assert!(
        spec.zones
            .iter()
            .any(|z| z.key_min == 46 && z.articulation.starts_with("Open"))
    );
    assert!(
        spec.zones
            .iter()
            .any(|z| z.key_min == 44 && z.articulation.starts_with("Pedal"))
    );
}

#[test]
fn parse_mm2_kick_styx_with_ml_layers() {
    use signal_sampler::LibrarySpec;
    let path = std::path::Path::new(
        "/run/media/AudioHaven/Sampled/Drum Kits/MM2_extracted/resources/samples/Modern & Massive 2/01_Kick/02_24x16'' Q Drum Co. Copper Kick/library.styx",
    );
    if !path.exists() {
        return;
    }
    let spec = LibrarySpec::from_file(path).expect("parse MM2 kick styx");
    assert!(!spec.zones.is_empty());
    // Multiple mics (close, sub, OH, room mono, room close, room far)
    assert!(spec.mics.len() >= 5);
    // Kick maps to GM 36
    assert!(spec.zones.iter().any(|z| z.key_min == 36));
    // (ML, V) flatten produces wider velocity coverage — at least one zone
    // should have vel_max > 100 (top end of the dynamic range)
    assert!(spec.zones.iter().any(|z| z.vel_max > 100));
}

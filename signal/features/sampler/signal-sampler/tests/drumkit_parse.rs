#[test]
fn parse_ggd_drumkit_styx() {
    use signal_sampler::LibrarySpec;
    let path = std::path::Path::new(
        "/run/media/AudioHaven/Sampled/Drum Kits/GGD Modern and Massive/Snare/Gretsch Bell Brass 14x6.5/library.styx",
    );
    if !path.exists() {
        return;
    }
    let spec = LibrarySpec::from_file(path).expect("parse drum kit styx");
    assert_eq!(spec.name, "Gretsch Bell Brass 14x6.5");
    assert_eq!(spec.vendor, "GGD");
    assert_eq!(spec.mics.len(), 7);
    assert!(spec.mics.iter().any(|m| m.id == "RoomMono"));
    assert!(!spec.zones.is_empty(), "zones empty");
    let z0 = &spec.zones[0];
    assert!(!z0.mic.is_empty(), "mic field missing");
    assert!(!z0.articulation.is_empty(), "articulation missing");
    // GM snare = D1 = 38
    assert!(
        spec.zones.iter().any(|z| z.key_min == 38),
        "no D1 snare zone"
    );
}

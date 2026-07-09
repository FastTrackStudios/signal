#[test]
fn parse_css_zones_styx() {
    use signal_sampler::LibrarySpec;
    let path = std::path::Path::new(
        "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings/zones.styx",
    );
    if !path.exists() {
        return;
    }
    let spec = LibrarySpec::from_file(path).expect("parse CSS zones styx");
    assert!(spec.zones.len() > 100_000);
    // CSS legato has directional zones
    assert!(spec.zones.iter().any(|z| z.direction == "up"));
    assert!(spec.zones.iter().any(|z| z.direction == "down"));
    // Sustains have dynamic labels
    assert!(spec.zones.iter().any(|z| z.dynamic == "ff"));
    assert!(spec.zones.iter().any(|z| z.dynamic == "p"));
    // 5 mics
    let mics: std::collections::HashSet<_> = spec.zones.iter().map(|z| z.mic.clone()).collect();
    assert_eq!(mics.len(), 5);
    assert!(mics.contains("Mix"));
}

#[test]
fn parse_css_descriptive_styx_mix_default() {
    use signal_sampler::LibrarySpec;
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let signal_root = std::path::Path::new(&manifest)
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let path = signal_root
        .parent()
        .unwrap()
        .join("sample-collector/specs/cinematic-strings.styx");
    if !path.exists() {
        return;
    }
    let spec = LibrarySpec::from_file(&path).expect("parse CSS descriptive spec");
    let mix = spec.mics.iter().find(|m| m.id == "Mix").expect("Mix mic");
    assert!(mix.default, "Mix mic should be marked default");
    let main = spec.mics.iter().find(|m| m.id == "Main").expect("Main mic");
    assert!(!main.default, "Main mic should NOT be default");
    // Manual-validated legato delays
    let le = spec.legato_engine.as_ref().unwrap();
    assert_eq!(
        le.expressive.as_ref().unwrap().delay_for_velocity(30),
        Some(333)
    );
    assert_eq!(
        le.expressive.as_ref().unwrap().delay_for_velocity(80),
        Some(250)
    );
    assert_eq!(
        le.expressive.as_ref().unwrap().delay_for_velocity(110),
        Some(100)
    );
    // Same velocity→speed direction as Expressive (softer = slower): low
    // velocity = medium (150 ms), high velocity = fast (100 ms). The spec was
    // corrected to match the CSS v1.7 manual (it used to be inverted).
    assert_eq!(
        le.low_latency.as_ref().unwrap().delay_for_velocity(30),
        Some(150)
    );
    assert_eq!(
        le.low_latency.as_ref().unwrap().delay_for_velocity(80),
        Some(100)
    );
}

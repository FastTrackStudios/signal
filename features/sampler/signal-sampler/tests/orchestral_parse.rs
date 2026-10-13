//! Parse round-trip checks across all orchestral library zone maps.
//!
//! Each test asserts the spec loads + has zones + carries the new
//! per-zone fields (mic / dynamic / articulation / direction).

fn try_load(path: &str) -> Option<signal_sampler::LibrarySpec> {
    let p = std::path::Path::new(path);
    if !p.exists() {
        return None;
    }
    Some(signal_sampler::LibrarySpec::from_file(p).expect("parse"))
}

#[test]
fn css_brass_zones() {
    let Some(s) = try_load(
        "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Brass/zones.styx",
    ) else {
        return;
    };
    assert!(s.zones.len() > 100_000);
    let mics: std::collections::HashSet<_> = s.zones.iter().map(|z| z.mic.clone()).collect();
    assert_eq!(mics.len(), 4); // Close, Main, Mix, Room
    assert!(mics.contains("Mix"));
}

#[test]
fn css_solo_strings_zones() {
    let Some(s) = try_load(
        "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Solo Strings/zones.styx",
    ) else {
        return;
    };
    assert!(s.zones.len() > 50_000);
    // Solo strings: 4 sections, all with directional legato
    assert!(s.zones.iter().any(|z| z.direction == "up"));
}

#[test]
fn css_woodwinds_zones() {
    let Some(s) = try_load(
        "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Woodwinds/zones.styx",
    ) else {
        return;
    };
    assert!(s.zones.len() > 100_000);
    let mics: std::collections::HashSet<_> = s.zones.iter().map(|z| z.mic.clone()).collect();
    assert_eq!(mics.len(), 5); // Woodwinds add OH
    assert!(mics.contains("OH"));
}

#[test]
fn css_piano_zones() {
    let Some(s) = try_load(
        "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Piano/zones.styx",
    ) else {
        return;
    };
    assert!(!s.zones.is_empty());
    // Piano-specific articulations
    assert!(s.zones.iter().any(|z| z.articulation == "Sustain"));
    assert!(s.zones.iter().any(|z| z.articulation == "Keyup"));
}

#[test]
fn pacific_brass_zones() {
    let Some(s) = try_load(
        "/run/media/AudioHaven/Sampled/Orchestral/Ocean Series/Pacific - Brass/zones.styx",
    ) else {
        return;
    };
    assert!(s.zones.len() > 10_000);
    let mics: std::collections::HashSet<_> = s.zones.iter().map(|z| z.mic.clone()).collect();
    assert!(mics.contains("Ambient"));
    assert!(mics.contains("Close"));
    // Pacific dynamics encode as dynN
    assert!(s.zones.iter().any(|z| z.dynamic.starts_with("dyn")));
}

#[test]
fn pacific_ensemble_strings_zones() {
    let Some(s) = try_load(
        "/run/media/AudioHaven/Sampled/Orchestral/Ocean Series/Pacific - Ensemble Strings/zones.styx",
    ) else {
        return;
    };
    assert!(s.zones.len() > 50_000);
    let secs: std::collections::HashSet<_> = s.zones.iter().map(|z| z.section.clone()).collect();
    // 7 sections (Cello, Contrabass, Ensemble, Harp, Viola, Violin, Violin Overtones)
    assert!(secs.len() >= 7);
}

#[test]
fn pacific_solo_strings_zones() {
    let Some(s) = try_load(
        "/run/media/AudioHaven/Sampled/Orchestral/Ocean Series/Pacific - Solo Strings/zones.styx",
    ) else {
        return;
    };
    assert!(s.zones.len() > 10_000);
    let mics: std::collections::HashSet<_> = s.zones.iter().map(|z| z.mic.clone()).collect();
    assert_eq!(mics.len(), 3); // Solo has 3 mics: Ambient, Close, Surround
}

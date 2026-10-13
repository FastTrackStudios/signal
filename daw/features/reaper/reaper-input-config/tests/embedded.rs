#[test]
fn the_shipped_profiles_are_embedded() {
    assert!(
        !reaper_input_config::PROFILES.is_empty(),
        "no profiles embedded"
    );
    assert!(
        reaper_input_config::PROFILES
            .iter()
            .any(|p| p.id == "fasttrackstudio"),
        "the fasttrackstudio profile is missing"
    );
    for p in reaper_input_config::PROFILES {
        assert!(
            !p.profile_styx.is_empty(),
            "{} has an empty profile.styx",
            p.id
        );
    }
    assert!(
        !reaper_input_config::WORKFLOWS.is_empty(),
        "no workflows embedded"
    );
    assert!(
        !reaper_input_config::OVERLAYS.is_empty(),
        "no overlays embedded"
    );
}

//! Integration coverage for Pro Tools alternate playlists becoming
//! REAPER fixed item lanes (audio path).
//!
//! `wonder-session` exercises the audio path: several tracks carry alternate
//! playlists that must land on dedicated fixed lanes. `routing-examples` is a
//! control: it has no alternate playlists, so the converter must not emit any
//! fixed-lane markup.
//!
//! No bundled fixture carries MIDI alternate playlists, so the MIDI lane path
//! is covered by a synthetic unit test in `daw_reaper::project_import`
//! (`midi_alternate_playlists_become_fixed_lanes` /
//! `midi_single_playlist_has_no_fixed_lanes`).

const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../dawfile/dawfile-protools/tests/fixtures/"
);

/// Count lines whose trimmed start is the given REAPER token (include the
/// trailing space when a bare prefix would collide with a longer token).
fn count_token(rpp: &str, token: &str) -> usize {
    rpp.lines()
        .filter(|l| l.trim_start().starts_with(token))
        .count()
}

/// Items placed on a fixed lane.
///
/// There is no per-item `LANE` token in REAPER's format — emitting one makes
/// REAPER reject the project. A fixed-lanes track divides its height evenly
/// and each item states its own slice as `YPOS <y> <height> <mode>`, which is
/// what the lane is read back out of.
fn count_item_lanes(rpp: &str) -> usize {
    rpp.lines()
        .filter(|l| l.trim_start().starts_with("YPOS "))
        .count()
}

#[test]
fn wonder_session_playlists_become_fixed_lanes() {
    let path = format!("{FIXTURES}wonder-session.ptx");
    let rpp = daw_reaper::project_import::protools_to_rpp(&path)
        .unwrap_or_else(|e| panic!("{path}: ptx→rpp failed: {e}"));

    assert!(
        count_token(&rpp, "FIXEDLANES") > 0,
        "wonder-session: expected FIXEDLANES markup"
    );
    assert!(
        count_item_lanes(&rpp) > 0,
        "wonder-session: expected per-item lane placement (YPOS)"
    );
    assert!(
        count_token(&rpp, "LANENAME ") > 0,
        "wonder-session: expected LANENAME markup"
    );
    // Every FIXEDLANES comp track also emits a LANEREC line.
    assert_eq!(
        count_token(&rpp, "LANEREC "),
        count_token(&rpp, "FIXEDLANES"),
        "every FIXEDLANES track must emit a LANEREC line"
    );
}

#[test]
fn routing_examples_has_no_fixed_lanes() {
    let path = format!("{FIXTURES}routing-examples.ptx");
    let rpp = daw_reaper::project_import::protools_to_rpp(&path)
        .unwrap_or_else(|e| panic!("{path}: ptx→rpp failed: {e}"));

    assert_eq!(
        count_token(&rpp, "FIXEDLANES"),
        0,
        "routing-examples has no alternate playlists; expected no FIXEDLANES"
    );
    assert_eq!(
        count_item_lanes(&rpp),
        0,
        "routing-examples: expected no per-item lane placement"
    );
    assert_eq!(
        count_token(&rpp, "LANENAME "),
        0,
        "routing-examples: expected no LANENAME"
    );
}

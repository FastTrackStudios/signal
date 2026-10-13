//! The reference session's group flags survive the round trip.
//!
//! `dynamic_template::golden_session` builds the maximal session
//! (`docs/spec/session/maximal-template.md`), where each instrument
//! folder is the VCA lead of its bus with mute and solo along for the
//! ride: `Electric` leads group 1, `ELECTRIC BUS` follows it. REAPER
//! stores that as one `GROUP_FLAGS` line of 25 bitmasks in an order
//! that is not the grouping dialog's — mute is fields 5/6, solo 7/8,
//! rec-arm 9/10, width only 19/20, VCA 21/22 — and for a long time the
//! writer had width at 5/6, which pushed every later pair by two: the
//! folder came out solo lead and rec-arm lead instead of mute/solo
//! lead (session #41).
//!
//! This builds the session and reads its output back through
//! `dawfile-reaper`'s `GROUP_FLAGS` parsing, landing on the fields
//! daw-standalone's `decode_grouping` reads, so the builder and the
//! reader cannot drift apart again without this noticing. (The
//! `GROUP_FLAGS` line is the builder's own — `dawfile-reaper` parses it
//! but does not write it yet — which is exactly why the read-back
//! matters.)

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

/// 1-based `GROUP_FLAGS` field numbers, as `decode_grouping` reads them
/// and as a REAPER-saved project confirms them.
const MUTE_LEAD: usize = 5;
const MUTE_FOLLOW: usize = 6;
const SOLO_LEAD: usize = 7;
const SOLO_FOLLOW: usize = 8;
const RECARM_LEAD: usize = 9;
const RECARM_FOLLOW: usize = 10;
const WIDTH_LEAD: usize = 19;
const WIDTH_FOLLOW: usize = 20;
const VCA_LEAD: usize = 21;
const VCA_FOLLOW: usize = 22;

/// The builder's output: the maximal session as project text.
fn template_rpp() -> Result<String> {
    Ok(dynamic_template::golden_session::build(&dynamic_template::golden_session::maximal()).rpp)
}

/// The groups (1-based numbers) whose bit is set in one field of a
/// track's parsed `GROUP_FLAGS`. Trailing zero fields are omitted by
/// REAPER and by the writer, so a missing field is an empty set.
fn groups_in(flags: &[u32], field: usize) -> Vec<u32> {
    let mask = field
        .checked_sub(1)
        .and_then(|i| flags.get(i))
        .copied()
        .unwrap_or(0);
    (1..=32)
        .filter(|group| mask & (1_u32 << (group - 1)) != 0)
        .collect()
}

fn group_flags<'a>(
    project: &'a dawfile_reaper::types::ReaperProject,
    name: &str,
) -> Result<&'a [u32]> {
    let track = project
        .tracks
        .iter()
        .find(|track| track.name == name)
        .ok_or_else(|| format!("no track named {name:?} in the template"))?;
    track
        .group_flags
        .as_deref()
        .ok_or_else(|| format!("{name:?} has no GROUP_FLAGS line").into())
}

#[test]
fn the_instrument_folder_is_vca_mute_and_solo_lead_of_its_bus() -> Result<()> {
    let project = dawfile_reaper::io::parse_project_text(&template_rpp()?)?;

    for (folder, bus, group) in [
        ("Electric", "ELECTRIC BUS", 1),
        ("Acoustic", "ACOUSTIC BUS", 2),
    ] {
        let lead = group_flags(&project, folder)?;
        assert_eq!(
            groups_in(lead, VCA_LEAD),
            [group],
            "{folder} is the VCA lead of group {group}"
        );
        assert_eq!(
            groups_in(lead, MUTE_LEAD),
            [group],
            "{folder} is the mute lead of group {group}"
        );
        assert_eq!(
            groups_in(lead, SOLO_LEAD),
            [group],
            "{folder} is the solo lead of group {group}"
        );
        // What the width-at-5/6 order produced: the folder arming the bus.
        assert!(
            groups_in(lead, RECARM_LEAD).is_empty(),
            "{folder} is not a rec-arm lead"
        );
        assert!(
            groups_in(lead, WIDTH_LEAD).is_empty(),
            "{folder} is not a width lead"
        );
        for follow in [
            MUTE_FOLLOW,
            SOLO_FOLLOW,
            RECARM_FOLLOW,
            WIDTH_FOLLOW,
            VCA_FOLLOW,
        ] {
            assert!(
                groups_in(lead, follow).is_empty(),
                "{folder} follows nothing"
            );
        }

        let follow = group_flags(&project, bus)?;
        assert_eq!(
            groups_in(follow, VCA_FOLLOW),
            [group],
            "{bus} is the VCA follow of group {group}"
        );
        assert_eq!(
            groups_in(follow, MUTE_FOLLOW),
            [group],
            "{bus} is the mute follow of group {group}"
        );
        assert_eq!(
            groups_in(follow, SOLO_FOLLOW),
            [group],
            "{bus} is the solo follow of group {group}"
        );
        assert!(
            groups_in(follow, RECARM_FOLLOW).is_empty(),
            "{bus} is not a rec-arm follow"
        );
        assert!(
            groups_in(follow, WIDTH_FOLLOW).is_empty(),
            "{bus} is not a width follow"
        );
        for lead in [MUTE_LEAD, SOLO_LEAD, RECARM_LEAD, WIDTH_LEAD, VCA_LEAD] {
            assert!(groups_in(follow, lead).is_empty(), "{bus} leads nothing");
        }
    }
    Ok(())
}

/// The reader itself, against lines REAPER saved
/// (`helgobox/resources/test-projects/issue-45-grouping-vca-test.RPP`):
/// a lead of everything in group 1 sets 1,3,…,13 and 19; its follower
/// sets 2,4,…,14 and 20. Only seven pairs before the reverse flags and
/// width at 19/20 fits that — the order the constants above encode.
#[test]
fn reaper_saved_lines_land_on_the_named_fields() -> Result<()> {
    let text = r#"<REAPER_PROJECT 0.1 "6.69+dev1102/macOS-arm64" 1667300000 0
  <TRACK {A0000000-0000-0000-0000-000000000001}
    NAME "Group 1 lead"
    GROUP_FLAGS 1 0 1 0 1 0 1 0 1 0 1 0 1 0 0 0 0 0 1
  >
  <TRACK {A0000000-0000-0000-0000-000000000002}
    NAME "Group 1 follow / 2 lead"
    GROUP_FLAGS 2 1 2 1 2 1 2 1 2 1 2 1 2 1 0 0 2 0 2 1
  >
  <TRACK {A0000000-0000-0000-0000-000000000003}
    NAME "Group 3 VCA lead"
    GROUP_FLAGS 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 4
  >
>
"#;
    let project = dawfile_reaper::io::parse_project_text(text)?;

    let lead = group_flags(&project, "Group 1 lead")?;
    for field in [MUTE_LEAD, SOLO_LEAD, RECARM_LEAD, WIDTH_LEAD] {
        assert_eq!(groups_in(lead, field), [1], "field {field} is a lead field");
    }
    for field in [
        MUTE_FOLLOW,
        SOLO_FOLLOW,
        RECARM_FOLLOW,
        WIDTH_FOLLOW,
        VCA_LEAD,
        VCA_FOLLOW,
    ] {
        assert!(
            groups_in(lead, field).is_empty(),
            "field {field} is not led by a lead-of-everything"
        );
    }

    let follow = group_flags(&project, "Group 1 follow / 2 lead")?;
    for field in [MUTE_FOLLOW, SOLO_FOLLOW, RECARM_FOLLOW, WIDTH_FOLLOW] {
        assert_eq!(
            groups_in(follow, field),
            [1],
            "field {field} follows group 1"
        );
    }
    for field in [MUTE_LEAD, SOLO_LEAD, RECARM_LEAD, WIDTH_LEAD] {
        assert_eq!(groups_in(follow, field), [2], "field {field} leads group 2");
    }

    let vca = group_flags(&project, "Group 3 VCA lead")?;
    assert_eq!(groups_in(vca, VCA_LEAD), [3]);
    assert!(groups_in(vca, VCA_FOLLOW).is_empty());
    Ok(())
}

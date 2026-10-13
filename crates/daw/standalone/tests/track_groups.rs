//! Track-group slots on the standalone backend: per-family lead/follow
//! flags over all 128 slots, slot names, and the free-slot probe. The
//! same calls run against REAPER in
//! `features/reaper/daw-reaper/tests/reaper_track_groups.rs`.

use daw_proto::project::ProjectContext;
use daw_proto::track::{
    GroupFamily, GroupFlagChange, GroupModifier, GroupModifierChange, GroupRole,
};
use daw_proto::{ProjectInfo, Projects, TrackRef, Tracks};
use daw_standalone::sync::Standalone;

fn seeded() -> (Standalone, ProjectContext) {
    let daw = Standalone::new();
    let guid = daw.seed_project(ProjectInfo {
        guid: "groups".into(),
        name: "groups".into(),
        path: String::new(),
    });
    (daw, ProjectContext::Project(guid))
}

fn flag(slot: u32, family: GroupFamily, role: GroupRole) -> GroupFlagChange {
    GroupFlagChange { slot, family, role }
}

/// A lead/follow pair on one family, written and read back through
/// the facade; the other families stay untouched.
#[test]
fn vca_lead_follow_pair_on_slot_128_reads_back() {
    let (daw, ctx) = seeded();
    let lead = Tracks::add(&daw, ctx.clone(), "FTS LANG EN", None).unwrap();
    let follower = Tracks::add(&daw, ctx.clone(), "Vox EN", None).unwrap();
    let lead_ref = TrackRef::Guid(lead.clone());
    let follower_ref = TrackRef::Guid(follower.clone());

    daw.set_group_flags(
        ctx.clone(),
        lead_ref.clone(),
        flag(128, GroupFamily::Vca, GroupRole::Lead),
    )
    .unwrap();
    daw.set_group_flags(
        ctx.clone(),
        follower_ref.clone(),
        flag(128, GroupFamily::Vca, GroupRole::Follow),
    )
    .unwrap();

    let lead_flags = daw.group_flags(ctx.clone(), lead_ref.clone()).unwrap();
    let follower_flags = daw.group_flags(ctx.clone(), follower_ref.clone()).unwrap();
    assert_eq!(lead_flags.role(GroupFamily::Vca, 128), GroupRole::Lead);
    assert_eq!(
        follower_flags.role(GroupFamily::Vca, 128),
        GroupRole::Follow
    );
    assert_eq!(
        lead_flags.role(GroupFamily::Vca, 127),
        GroupRole::None,
        "neighbour slot untouched"
    );
    for fam in GroupFamily::ALL
        .into_iter()
        .filter(|f| *f != GroupFamily::Vca)
    {
        assert_eq!(
            lead_flags.role(fam, 128),
            GroupRole::None,
            "{fam:?} untouched on the lead"
        );
        assert_eq!(
            follower_flags.role(fam, 128),
            GroupRole::None,
            "{fam:?} untouched on the follower"
        );
    }

    // The same masks ride `Track::grouping`.
    let t = Tracks::get(&daw, ctx.clone(), follower_ref.clone()).unwrap();
    assert_eq!(t.grouping, follower_flags);

    // `None` takes the track out of the family; lead → follow swaps the bit.
    daw.set_group_flags(
        ctx.clone(),
        lead_ref.clone(),
        flag(128, GroupFamily::Vca, GroupRole::Follow),
    )
    .unwrap();
    let swapped = daw.group_flags(ctx.clone(), lead_ref.clone()).unwrap();
    assert_eq!(swapped.role(GroupFamily::Vca, 128), GroupRole::Follow);
    assert_eq!(swapped.vca_lead, 0);
    daw.set_group_flags(
        ctx.clone(),
        lead_ref.clone(),
        flag(128, GroupFamily::Vca, GroupRole::None),
    )
    .unwrap();
    assert!(daw.group_flags(ctx, lead_ref).unwrap().is_empty());
}

#[test]
fn every_family_and_modifier_round_trips() {
    let (daw, ctx) = seeded();
    let t = TrackRef::Guid(Tracks::add(&daw, ctx.clone(), "T", None).unwrap());
    for (i, fam) in GroupFamily::ALL.into_iter().enumerate() {
        let slot = 60 + i as u32; // straddles the 64/65 boundary
        daw.set_group_flags(ctx.clone(), t.clone(), flag(slot, fam, GroupRole::Lead))
            .unwrap();
    }
    for (i, m) in GroupModifier::ALL.into_iter().enumerate() {
        let slot = 60 + i as u32;
        daw.set_group_modifier(
            ctx.clone(),
            t.clone(),
            GroupModifierChange {
                slot,
                modifier: m,
                enabled: true,
            },
        )
        .unwrap();
    }
    let g = daw.group_flags(ctx.clone(), t.clone()).unwrap();
    for (i, fam) in GroupFamily::ALL.into_iter().enumerate() {
        assert_eq!(g.role(fam, 60 + i as u32), GroupRole::Lead, "{fam:?}");
        assert_eq!(
            g.role(fam, 61 + i as u32),
            GroupRole::None,
            "{fam:?} next slot"
        );
    }
    for (i, m) in GroupModifier::ALL.into_iter().enumerate() {
        assert!(g.modifier(m, 60 + i as u32), "{m:?}");
        assert!(!g.modifier(m, 61 + i as u32), "{m:?} next slot");
    }
    daw.set_group_modifier(
        ctx.clone(),
        t.clone(),
        GroupModifierChange {
            slot: 60,
            modifier: GroupModifier::VolumeReverse,
            enabled: false,
        },
    )
    .unwrap();
    assert!(
        !daw.group_flags(ctx, t)
            .unwrap()
            .modifier(GroupModifier::VolumeReverse, 60)
    );
}

/// Out-of-range slots are refused, not silently dropped.
#[test]
fn slot_zero_and_129_are_errors() {
    let (daw, ctx) = seeded();
    let t = TrackRef::Guid(Tracks::add(&daw, ctx.clone(), "T", None).unwrap());
    assert!(
        daw.set_group_flags(
            ctx.clone(),
            t.clone(),
            flag(0, GroupFamily::Mute, GroupRole::Lead)
        )
        .is_err()
    );
    assert!(
        daw.set_group_flags(
            ctx.clone(),
            t.clone(),
            flag(129, GroupFamily::Mute, GroupRole::Lead)
        )
        .is_err()
    );
    assert!(daw.group_flags(ctx, t).unwrap().is_empty());
}

/// A slot used by any family — not only a VCA lead — is not free.
#[test]
fn first_free_group_slot_sees_every_family() {
    let (daw, ctx) = seeded();
    let t = TrackRef::Guid(Tracks::add(&daw, ctx.clone(), "T", None).unwrap());
    assert_eq!(daw.first_free_group_slot(ctx.clone(), 126, 128), Some(126));
    daw.set_group_flags(
        ctx.clone(),
        t.clone(),
        flag(126, GroupFamily::Mute, GroupRole::Follow),
    )
    .unwrap();
    assert_eq!(
        daw.first_free_group_slot(ctx.clone(), 126, 128),
        Some(127),
        "mute-follow occupies 126"
    );
    daw.set_group_modifier(
        ctx.clone(),
        t.clone(),
        GroupModifierChange {
            slot: 127,
            modifier: GroupModifier::NoLeadWhenFollow,
            enabled: true,
        },
    )
    .unwrap();
    assert_eq!(
        daw.first_free_group_slot(ctx.clone(), 126, 128),
        Some(128),
        "a modifier occupies 127"
    );
    daw.set_group_flags(ctx.clone(), t, flag(128, GroupFamily::Vca, GroupRole::Lead))
        .unwrap();
    assert_eq!(
        daw.first_free_group_slot(ctx.clone(), 126, 128),
        None,
        "band full"
    );
    assert_eq!(
        daw.first_free_group_slot(ctx, 129, 130),
        None,
        "outside the range"
    );
}

/// `set_group_membership` (the mutual all-families member) is the
/// composition of the per-family setter, and can be undone by it.
#[test]
fn mutual_membership_is_every_family_lead_and_follow() {
    let (daw, ctx) = seeded();
    let t = TrackRef::Guid(Tracks::add(&daw, ctx.clone(), "T", None).unwrap());
    daw.set_group_membership(ctx.clone(), t.clone(), 7, true)
        .unwrap();
    let g = daw.group_flags(ctx.clone(), t.clone()).unwrap();
    for fam in GroupFamily::ALL {
        assert_ne!(g.lead(fam) & (1u128 << 6), 0, "{fam:?} lead");
        assert_ne!(g.follow(fam) & (1u128 << 6), 0, "{fam:?} follow");
    }
    daw.set_group_membership(ctx.clone(), t.clone(), 7, false)
        .unwrap();
    assert!(daw.group_flags(ctx, t).unwrap().is_empty());
}

/// Slot names read back through the same project-info key REAPER
/// uses, so a watcher recognises its slots on either backend.
#[test]
fn slot_128_name_reads_back() {
    let (daw, ctx) = seeded();
    daw.set_group_name(ctx.clone(), 128, "FTS LANG EN").unwrap();
    assert_eq!(
        daw.get_project_info_string(ctx.clone(), "TRACK_GROUP_NAME:128"),
        "FTS LANG EN"
    );
    assert_eq!(
        daw.get_project_info_string(ctx.clone(), "TRACK_GROUP_NAME:127"),
        "",
        "neighbour unnamed"
    );
    assert!(daw.set_group_name(ctx, 129, "x").is_err());
}

#[cfg(all(feature = "rpp-save", feature = "rpp-loader"))]
/// Acceptance: a lead/follow pair written through the facade survives
/// a trip out through the real file writer and back in through the
/// loader. Before this, the writer emitted no `GROUP_FLAGS` at all, so
/// a group made in the session was dropped on every save.
#[test]
fn group_flags_survive_the_real_file_round_trip() {
    use dawfile_standalone::project::DawProject;

    // The grouping to save, built through the facade's own setter.
    let (daw, ctx) = seeded();
    let lead = TrackRef::Guid(Tracks::add(&daw, ctx.clone(), "FTS VCA GTR", None).unwrap());
    let follower = TrackRef::Guid(Tracks::add(&daw, ctx.clone(), "GTR BUS", None).unwrap());
    // Slot 3 lands in the low line, 33 in `_HIGH` — both sides of the
    // 32-slot boundary in one project.
    for (track, role) in [(&lead, GroupRole::Lead), (&follower, GroupRole::Follow)] {
        daw.set_group_flags(ctx.clone(), track.clone(), flag(3, GroupFamily::Vca, role))
            .unwrap();
        daw.set_group_flags(
            ctx.clone(),
            track.clone(),
            flag(33, GroupFamily::Mute, role),
        )
        .unwrap();
    }
    let want_lead = daw.group_flags(ctx.clone(), lead).unwrap();
    let want_follower = daw.group_flags(ctx, follower).unwrap();
    assert_eq!(
        want_lead.slots_beyond_rpp(),
        0,
        "this fixture must be file-representable, or it proves nothing"
    );

    // An ungrouped file, as a project that has never heard of groups.
    let ungrouped = "<REAPER_PROJECT 0.1 \"7.75/linux-x86_64\" 1700000000\n         \x20 <TRACK {CCCCCCCC-0000-0000-0000-000000000001}\n         \x20   NAME \"FTS VCA GTR\"\n         \x20   TRACKID {CCCCCCCC-0000-0000-0000-000000000001}\n         \x20 >\n         \x20 <TRACK {CCCCCCCC-0000-0000-0000-000000000002}\n         \x20   NAME \"GTR BUS\"\n         \x20   TRACKID {CCCCCCCC-0000-0000-0000-000000000002}\n         \x20 >\n         >\n";
    let (mut project, _) = DawProject::import_rpp(ungrouped, "T").expect("import");
    assert!(
        !project
            .to_rpp_patched()
            .expect("export")
            .0
            .contains("GROUP_FLAGS"),
        "the fixture starts ungrouped — the control for the assertion below"
    );

    // Group it, save it.
    project.edit(|doc| {
        doc.tracks[0].track.grouping = want_lead.clone();
        doc.tracks[1].track.grouping = want_follower.clone();
    });
    let (text, report) = project.to_rpp_patched().expect("export");
    assert!(
        text.contains("GROUP_FLAGS ") && text.contains("GROUP_FLAGS_HIGH "),
        "the writer must emit both lines: {text}"
    );
    assert_eq!(
        report
            .changes
            .iter()
            .filter(|c| c.contains("groups: 0 lines \u{2192} 2 lines"))
            .count(),
        2,
        "both tracks' group lines must be reported, not written silently: {:?}",
        report.changes
    );

    // …and back in through the loader.
    let reloaded = Standalone::new();
    let summary =
        daw_standalone::project_loader::load_rpp_text(&reloaded, "T", "/tmp/g.rpp", &text)
            .expect("load");
    let tracks = Tracks::all(
        &reloaded,
        ProjectContext::Project(summary.project_guid.clone()),
    );
    assert_eq!(tracks[0].grouping, want_lead, "lead round-trips");
    assert_eq!(tracks[1].grouping, want_follower, "follower round-trips");
}

/// A slot above 64 has no `.RPP` line to go in. The facade accepts it
/// (the live REAPER API reaches all 128) but a caller that saves must
/// be able to see the loss coming.
#[test]
fn slots_above_64_are_reported_as_unwritable() {
    let (daw, ctx) = seeded();
    let t = TrackRef::Guid(Tracks::add(&daw, ctx.clone(), "T", None).unwrap());
    daw.set_group_flags(
        ctx.clone(),
        t.clone(),
        flag(128, GroupFamily::Vca, GroupRole::Lead),
    )
    .unwrap();
    let g = daw.group_flags(ctx.clone(), t.clone()).unwrap();
    assert_eq!(g.slots_beyond_rpp(), 1u128 << 127);
    assert_eq!(
        g.to_rpp_fields(),
        (Vec::new(), Vec::new()),
        "nothing to write"
    );

    daw.set_group_flags(
        ctx.clone(),
        t.clone(),
        flag(64, GroupFamily::Vca, GroupRole::Lead),
    )
    .unwrap();
    let g = daw.group_flags(ctx, t).unwrap();
    assert_eq!(g.slots_beyond_rpp(), 1u128 << 127, "slot 64 is writable");
    assert!(
        !g.to_rpp_fields().1.is_empty(),
        "slot 64 rides the high line"
    );
}

/// An unedited export of a file that already carries group lines must
/// not touch them — not their text, and not their position among the
/// track's other lines.
///
/// This is the guard on where `patch_group_flags` puts them. REAPER
/// writes `GROUP_FLAGS` between `VU` and `TRACKHEIGHT`, after the lane
/// lines; a resolution that appended them instead would pass every
/// round-trip test above and still rewrite every grouped session on
/// save.
#[cfg(all(feature = "rpp-save", feature = "rpp-loader"))]
#[test]
fn an_unedited_grouped_track_is_not_rewritten() {
    use dawfile_standalone::project::DawProject;

    // The line order REAPER itself writes (verified against a 7.66
    // session), lanes and groups both present.
    let grouped = "<REAPER_PROJECT 0.1 \"7.75/linux-x86_64\" 1700000000\n\
         \x20 <TRACK {DDDDDDDD-0000-0000-0000-000000000001}\n\
         \x20   NAME Kick\n\
         \x20   PEAKCOL 16576\n\
         \x20   VOLPAN 1 0 -1 -1 1\n\
         \x20   MUTESOLO 0 0 0\n\
         \x20   IPHASE 0\n\
         \x20   ISBUS 0 0\n\
         \x20   FREEMODE 2\n\
         \x20   FIXEDLANES 1 0 0 0 0\n\
         \x20   SEL 0\n\
         \x20   REC 0 -1 1 0 0 0 0 0\n\
         \x20   VU 64\n\
         \x20   GROUP_FLAGS 0 0 0 0 0 0 0 0 6 0 0 0 0 0 0 0 0 0 0 0 1\n\
         \x20   TRACKHEIGHT 0 0 0 0 0 0 0\n\
         \x20   NCHAN 2\n\
         \x20   TRACKID {DDDDDDDD-0000-0000-0000-000000000001}\n\
         \x20 >\n\
         >\n";

    let (project, _) = DawProject::import_rpp(grouped, "T").expect("import");
    let (text, report) = project.to_rpp_patched().expect("export");

    let group_changes: Vec<&String> = report
        .changes
        .iter()
        .filter(|c| c.contains("group") || c.contains("GROUP_FLAGS"))
        .collect();
    assert!(
        group_changes.is_empty(),
        "an unedited grouped track must report no group change: {group_changes:?}"
    );

    // And the line is still where REAPER put it.
    let keys: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    let at = |k: &str| keys.iter().position(|x| *x == k);
    let (vu, group, height) = (
        at("VU").expect("VU"),
        at("GROUP_FLAGS").expect("GROUP_FLAGS survives"),
        at("TRACKHEIGHT").expect("TRACKHEIGHT"),
    );
    assert!(
        vu < group && group < height,
        "GROUP_FLAGS must stay between VU and TRACKHEIGHT, got {keys:?}"
    );
    assert!(
        at("FIXEDLANES").expect("FIXEDLANES") < group,
        "the lane lines come before the group lines"
    );
}

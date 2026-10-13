//! Track groups inside a real REAPER (#52).
//!
//! Two of these settle questions no source read could answer, and their
//! results are recorded on the ticket:
//!
//! - `slot_128_name_round_trips` — the SDK header still documents
//!   `TRACK_GROUP_NAME:X` as `X should be 1..64`
//!   (`reaper_plugin_functions.h:3073`) while REAPER 7.23+ has 128
//!   slots. If naming 65–128 fails, FTS's downward slot walk starts at
//!   64 instead of 128.
//! - `vca_lead_mute_does_not_move_followers_button` — whether muting a
//!   VCA lead makes REAPER *report* its followers as muted. It does
//!   not: VCA is playback-only, so a switch that needs the mute to be
//!   readable state gangs it through `GroupFamily::Mute` instead.
//!
//! Run with: `cargo run -p daw-reaper-xtask -- reaper_track_groups`

use daw::test::reaper_test;
use daw_proto::track::{GroupFamily, GroupRole};

/// Name the top slot and read it back through the same project-info key.
/// Slot 1 is the negative control: naming works at all in this rig.
#[reaper_test(isolated)]
async fn slot_128_name_round_trips(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tracks = project.tracks();

    tracks.set_group_name(1, "FTS CONTROL 1").await?;
    let low = project.get_info_string("TRACK_GROUP_NAME:1").await?;
    assert_eq!(
        low, "FTS CONTROL 1",
        "naming slot 1 must work — if this fails the rig is wrong, not the slot range"
    );

    tracks.set_group_name(128, "FTS LANG EN").await?;
    let high = project.get_info_string("TRACK_GROUP_NAME:128").await?;
    assert_eq!(
        high, "FTS LANG EN",
        "TRACK_GROUP_NAME:128 did not round-trip — the SDK header's 1..64 \
         still holds, so the FTS slot walk starts at 64 (session#52, #55)"
    );

    Ok(())
}

/// Mute a VCA lead and read the follower's effective mute.
///
/// OBSERVED (REAPER 7.75, 2026-09-14): the follower is still reported
/// UNMUTED. `Track::muted` on this backend is `GetTrackUIMute`, which
/// is REAPER's own effective reading, and it does not move — consistent
/// with VCA being a playback-time scaling (user guide §5.16) that never
/// touches a follower's controls, the same way a VCA lead's fader move
/// leaves follower faders where they are.
///
/// What this settles for session#60: the language switch cannot read a
/// follower's mute back to know a language is off. If the switch needs
/// mute to be *state* (readable, saved, visible), it gangs it through
/// `GroupFamily::Mute`, whose lead/follow does move the follower's own
/// button on both backends. What it does NOT settle: whether REAPER's
/// audio is silenced for the follower — this harness has no render read
/// on the REAPER backend, and nothing here was measured about audio.
#[reaper_test(isolated)]
async fn vca_lead_mute_does_not_move_followers_button(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tracks = project.tracks();
    let lead = tracks.add("FTS LANG EN", None).await?;
    let follower = tracks.add("Vox EN", None).await?;

    lead.set_group_flags(128, GroupFamily::Vca, GroupRole::Lead)
        .await?;
    follower
        .set_group_flags(128, GroupFamily::Vca, GroupRole::Follow)
        .await?;

    // The flags are really on the tracks (else the mute reading below
    // would prove nothing).
    let lead_flags = lead.group_flags().await?;
    let follower_flags = follower.group_flags().await?;
    assert_eq!(
        lead_flags.role(GroupFamily::Vca, 128),
        GroupRole::Lead,
        "VCA lead flag did not stick on slot 128"
    );
    assert_eq!(
        follower_flags.role(GroupFamily::Vca, 128),
        GroupRole::Follow,
        "VCA follow flag did not stick on slot 128"
    );

    assert!(!follower.is_muted().await?, "follower starts unmuted");
    lead.mute().await?;
    let follower_muted_under_vca = follower.is_muted().await?;
    let lead_muted = lead.is_muted().await?;
    lead.unmute().await?;

    assert!(lead_muted, "the lead itself must report muted");
    assert!(
        !follower_muted_under_vca,
        "a VCA lead's mute did NOT reach the follower's reported mute — \
         if this ever starts passing, REAPER changed and session#60 can \
         use the VCA family for the language switch"
    );

    // Same gesture through the MUTE family, the mechanism #60 would fall
    // back to. It does not reach the follower either — and the reason is
    // ours, not REAPER's: `Tracks::set_muted` on this backend calls
    // `Track::mute(GangBehavior::DenyGang, GroupingBehavior::PreventGrouping)`,
    // so the facade's mute deliberately exercises NO gang, of any
    // family. A language switch built on the mute gang therefore has to
    // write every member's mute itself, or that decision has to be
    // revisited first (session#52, #60).
    lead.set_group_flags(128, GroupFamily::Vca, GroupRole::None)
        .await?;
    follower
        .set_group_flags(128, GroupFamily::Vca, GroupRole::None)
        .await?;
    lead.set_group_flags(127, GroupFamily::Mute, GroupRole::Lead)
        .await?;
    follower
        .set_group_flags(127, GroupFamily::Mute, GroupRole::Follow)
        .await?;
    // The control for both readings: the gang really is wired, so a
    // false above is REAPER's answer and not an empty setup.
    assert_eq!(
        lead.group_flags().await?.role(GroupFamily::Mute, 127),
        GroupRole::Lead
    );
    assert_eq!(
        follower.group_flags().await?.role(GroupFamily::Mute, 127),
        GroupRole::Follow
    );
    lead.mute().await?;
    let follower_muted_under_mute_gang = follower.is_muted().await?;
    lead.unmute().await?;
    assert!(
        !follower_muted_under_mute_gang,
        "the facade's set_muted passes PreventGrouping — if this starts \
         failing, that choice changed and #60's fallback works as written"
    );

    Ok(())
}

/// Per-family writes stay per-family: a mute-follow does not make the
/// track a volume follower, and the flags survive a read-back through
/// the live membership API on a slot above 64.
#[reaper_test(isolated)]
async fn per_family_flags_are_independent(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Gang", None).await?;

    track
        .set_group_flags(100, GroupFamily::Mute, GroupRole::Follow)
        .await?;
    let flags = track.group_flags().await?;
    assert_eq!(flags.role(GroupFamily::Mute, 100), GroupRole::Follow);
    for family in GroupFamily::ALL
        .into_iter()
        .filter(|f| *f != GroupFamily::Mute)
    {
        assert_eq!(
            flags.role(family, 100),
            GroupRole::None,
            "{family:?} must not be set by a Mute write"
        );
    }
    assert_eq!(
        flags.role(GroupFamily::Mute, 99),
        GroupRole::None,
        "neighbouring slot untouched"
    );

    track
        .set_group_flags(100, GroupFamily::Mute, GroupRole::None)
        .await?;
    assert!(
        track.group_flags().await?.is_empty(),
        "GroupRole::None clears the membership"
    );

    Ok(())
}

/// `first_free_group_slot` must see a slot occupied by any family, not
/// only a VCA lead (the old probe's blind spot).
#[reaper_test(isolated)]
async fn first_free_slot_sees_a_mute_only_group(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tracks = project.tracks();
    let track = tracks.add("Gang", None).await?;

    assert_eq!(
        tracks.first_free_group_slot(120, 121).await?,
        Some(120),
        "band starts free"
    );
    track
        .set_group_flags(120, GroupFamily::Mute, GroupRole::Lead)
        .await?;
    assert_eq!(
        tracks.first_free_group_slot(120, 121).await?,
        Some(121),
        "a mute-only group occupies its slot"
    );

    Ok(())
}

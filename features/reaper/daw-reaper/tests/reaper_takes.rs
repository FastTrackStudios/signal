//! Integration tests for the Take service (#25).
//!
//! Round-trip coverage for the chunk/source pieces wired in #25:
//! - add take → preserve_pitch toggle → re-read survives
//! - source detection on a stock MIDI take
//! - set_source_file on a generated WAV → source_type flips Audio
//! - delete_take via action 40129 → take_count decreases by one
//!
//! Run with: `cargo xtask reaper-test -- reaper_takes`

use daw::test::reaper_test;
use daw_proto::SourceType;
use std::fs;
use std::path::PathBuf;

/// Write a 0.1-second 44.1k mono silent PCM-16 WAV to `/tmp` and return
/// the path. Hand-rolled — we don't want a `hound`/`tempfile` dep just
/// for one test.
fn write_silence_wav(name: &str) -> PathBuf {
    let sample_rate: u32 = 44_100;
    let num_samples: u32 = 4_410; // 0.1s
    let bits_per_sample: u16 = 16;
    let channels: u16 = 1;
    let byte_rate = sample_rate * (bits_per_sample as u32 / 8) * channels as u32;
    let block_align = channels * bits_per_sample / 8;
    let data_size = num_samples * block_align as u32;
    let chunk_size = 36 + data_size;

    let mut buf: Vec<u8> = Vec::with_capacity(44 + data_size as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&chunk_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());
    buf.extend(std::iter::repeat_n(0u8, data_size as usize));

    let path = std::env::temp_dir().join(format!("daw-take-test-{name}.wav"));
    fs::write(&path, &buf).expect("write tmp wav");
    path
}

#[reaper_test(isolated)]
async fn take_delete_decreases_count(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Take Delete Track", None).await?;
    let item = track
        .items()
        .add(
            daw_proto::PositionInSeconds::from_seconds(0.0),
            daw_proto::Duration::from_seconds(2.0),
        )
        .await?;

    // Items start with one take. Add a second take so we have something
    // to delete without leaving the item empty.
    item.takes().add().await?;
    let before = item.info().await?.take_count;
    assert!(
        before >= 2,
        "expected at least 2 takes after add(), got {before}"
    );

    // Delete take #1 (index 1) via the new chunk-action path.
    let take_to_kill = item
        .takes()
        .by_index(1)
        .await?
        .ok_or_else(|| eyre::eyre!("take #1 missing pre-delete"))?;
    take_to_kill.delete().await?;

    let after = item.info().await?.take_count;
    assert_eq!(
        after,
        before - 1,
        "delete_take should decrement take_count: {before} → {after}"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn take_preserve_pitch_round_trips(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Preserve Pitch Track", None).await?;
    let item = track
        .items()
        .add(
            daw_proto::PositionInSeconds::from_seconds(0.0),
            daw_proto::Duration::from_seconds(1.0),
        )
        .await?;
    let take = item.takes().active().await?;

    take.set_preserve_pitch(true).await?;
    let info_on = take.info().await?;
    assert!(
        info_on.preserve_pitch,
        "preserve_pitch should be true after set(true)"
    );

    take.set_preserve_pitch(false).await?;
    let info_off = take.info().await?;
    assert!(
        !info_off.preserve_pitch,
        "preserve_pitch should be false after set(false)"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn take_set_source_file_flips_to_audio(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let wav = write_silence_wav("source_swap");
    let path_str = wav.to_string_lossy().into_owned();

    let project = ctx.project().clone();
    let track = project.tracks().add("Source Swap Track", None).await?;
    let item = track
        .items()
        .add(
            daw_proto::PositionInSeconds::from_seconds(0.0),
            daw_proto::Duration::from_seconds(0.5),
        )
        .await?;
    let take = item.takes().active().await?;

    take.set_source_file(&path_str).await?;
    let kind = take.source_type().await?;
    assert!(
        matches!(kind, SourceType::Audio),
        "after set_source_file('{path_str}'), source_type should be Audio, got {kind:?}"
    );

    let _ = fs::remove_file(&wav);
    Ok(())
}

#[reaper_test(isolated)]
async fn take_marker_add_list_update_delete(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Take Marker Track", None).await?;
    let item = track
        .items()
        .add(
            daw_proto::PositionInSeconds::from_seconds(0.0),
            daw_proto::Duration::from_seconds(4.0),
        )
        .await?;
    let take = item.takes().active().await?;

    // Add three markers at different source-PPQ positions.
    let m0 = take
        .add_marker("intro", 0.0, None)
        .await?
        .ok_or_else(|| eyre::eyre!("add_marker(intro) returned None"))?;
    let m1 = take
        .add_marker("verse", 960.0, Some(0xFF0000))
        .await?
        .ok_or_else(|| eyre::eyre!("add_marker(verse) returned None"))?;
    let m2 = take
        .add_marker("chorus", 1920.0, None)
        .await?
        .ok_or_else(|| eyre::eyre!("add_marker(chorus) returned None"))?;

    // List should return three markers.
    let markers = take.markers().await?;
    assert_eq!(
        markers.len(),
        3,
        "expected 3 markers, got {}",
        markers.len()
    );

    // Names + positions round-trip.
    let names: Vec<&str> = markers.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"intro"));
    assert!(names.contains(&"verse"));
    assert!(names.contains(&"chorus"));
    let verse = markers.iter().find(|m| m.name == "verse").unwrap();
    assert!(
        (verse.source_position_seconds - 960.0).abs() < 0.5,
        "verse position should be ~960 PPQ, got {}",
        verse.source_position_seconds
    );
    assert_eq!(
        verse.color,
        Some(0xFF0000),
        "verse color should round-trip; got {:?}",
        verse.color
    );

    // Update the first marker — rename + reposition.
    take.update_marker(m0, Some("intro-renamed"), Some(120.0), None)
        .await?;
    let after_update = take.markers().await?;
    let intro = after_update
        .iter()
        .find(|m| m.name == "intro-renamed")
        .ok_or_else(|| eyre::eyre!("renamed intro not found: {after_update:?}"))?;
    assert!(
        (intro.source_position_seconds - 120.0).abs() < 0.5,
        "intro should reposition to 120 PPQ"
    );

    // Delete the highest-index marker (chorus = m2).
    take.delete_marker(m2).await?;
    let after_delete = take.markers().await?;
    assert_eq!(
        after_delete.len(),
        2,
        "after delete should have 2 markers, got {}",
        after_delete.len()
    );
    assert!(after_delete.iter().all(|m| m.name != "chorus"));

    let _ = m1; // m1 is referenced — silence unused warnings
    Ok(())
}

#[reaper_test(isolated)]
async fn take_marker_at_project_position_inside_and_outside(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    use daw_proto::{Position, PositionInSeconds};

    let project = ctx.project().clone();
    let track = project.tracks().add("Take Marker At Track", None).await?;
    // Item starts at 4.0s, length 2.0s → playable region [4.0, 6.0].
    let item = track
        .items()
        .add(
            PositionInSeconds::from_seconds(4.0),
            daw_proto::Duration::from_seconds(2.0),
        )
        .await?;
    let take = item.takes().active().await?;

    // Inside the item: project_time 5.0s → source_time = 0 + (5.0 - 4.0) * 1 = 1.0s.
    let inside = Position::from_time(PositionInSeconds::from_seconds(5.0));
    let idx = take
        .add_marker_at(inside, "[alice] inside", Some(0xFF8800))
        .await?
        .ok_or_else(|| eyre::eyre!("inside marker should have been added"))?;
    let markers = take.markers().await?;
    let marker = markers
        .iter()
        .find(|m| m.index == idx)
        .ok_or_else(|| eyre::eyre!("added marker missing from list"))?;
    assert!(
        (marker.source_position_seconds - 1.0).abs() < 0.005,
        "expected source_position ~= 1.0s, got {}",
        marker.source_position_seconds
    );
    assert_eq!(marker.name, "[alice] inside");
    assert_eq!(marker.color, Some(0xFF8800));

    // Before the item: project_time 1.0s — outside, should be a no-op.
    let before = Position::from_time(PositionInSeconds::from_seconds(1.0));
    let outside_before = take.add_marker_at(before, "[alice] before", None).await?;
    assert!(
        outside_before.is_none(),
        "marker before item should be rejected, got idx={outside_before:?}"
    );

    // After the item: project_time 100.0s — also a no-op.
    let after = Position::from_time(PositionInSeconds::from_seconds(100.0));
    let outside_after = take.add_marker_at(after, "[alice] after", None).await?;
    assert!(
        outside_after.is_none(),
        "marker after item should be rejected, got idx={outside_after:?}"
    );

    // Still only one marker (the inside one).
    let final_markers = take.markers().await?;
    assert_eq!(
        final_markers.len(),
        1,
        "expected 1 marker after bounds-rejected calls, got {}",
        final_markers.len()
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn take_rating_up_rank_progression_and_clear(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Take Rating Track", None).await?;
    let item = track
        .items()
        .add(
            daw_proto::PositionInSeconds::from_seconds(0.0),
            daw_proto::Duration::from_seconds(2.0),
        )
        .await?;
    let take = item.takes().active().await?;

    // Fresh take is unranked.
    assert_eq!(
        take.rating().await?,
        None,
        "freshly created take should be unranked"
    );

    // First up-rank → level 1 (REAPER writes marker name `:)`).
    take.up_rank().await?;
    assert_eq!(
        take.rating().await?,
        Some(daw_proto::TakeRating::UpRank(1)),
        "after first up_rank, rating should be UpRank(1)"
    );

    // Second up-rank → level 2.
    take.up_rank().await?;
    assert_eq!(
        take.rating().await?,
        Some(daw_proto::TakeRating::UpRank(2)),
        "after second up_rank, rating should be UpRank(2)"
    );

    // Clear wipes ranking back to None.
    take.clear_rating().await?;
    assert_eq!(
        take.rating().await?,
        None,
        "after clear_rating, take should be unranked"
    );

    Ok(())
}

#[test]
fn take_rating_marker_name_round_trip() {
    use daw_proto::TakeRating;
    // Up-rank levels 1..=5 round-trip cleanly.
    for level in 1..=5u8 {
        let r = TakeRating::UpRank(level);
        let name = r.to_marker_name();
        assert_eq!(name.as_bytes()[0], b':');
        assert_eq!(name.len(), 1 + level as usize);
        assert_eq!(TakeRating::from_marker_name(&name), Some(r));
    }
    // Down-rank is a single level.
    assert_eq!(TakeRating::DownRank.to_marker_name(), ":(");
    assert_eq!(
        TakeRating::from_marker_name(":("),
        Some(TakeRating::DownRank)
    );
    // Multiple frownies still parse as DownRank (REAPER never writes >1
    // but we accept what we read).
    assert_eq!(
        TakeRating::from_marker_name(":((("),
        Some(TakeRating::DownRank)
    );
    // Ordinary marker names don't match.
    assert_eq!(TakeRating::from_marker_name("intro"), None);
    assert_eq!(TakeRating::from_marker_name(""), None);
    assert_eq!(TakeRating::from_marker_name(":"), None);
    assert_eq!(TakeRating::from_marker_name(":)abc"), None);
}

#[reaper_test(isolated)]
async fn take_source_type_detected_for_default(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Source Type Track", None).await?;
    let item = track
        .items()
        .add(
            daw_proto::PositionInSeconds::from_seconds(0.0),
            daw_proto::Duration::from_seconds(1.0),
        )
        .await?;
    let take = item.takes().active().await?;

    // The detector must classify the source as something concrete —
    // never `Unknown`. The exact type depends on REAPER's default for
    // a freshly-added item (typically MIDI on Linux), so we accept
    // any of the live variants.
    let kind = take.source_type().await?;
    assert!(
        matches!(
            kind,
            SourceType::Empty | SourceType::Audio | SourceType::Midi | SourceType::Video
        ),
        "default take source_type should be detected, got {kind:?}"
    );

    Ok(())
}

//! REAPER integration tests for velocity shaping.
//!
//! Everything in `midi-tools` is unit-tested against `&[Note]`, which
//! proves the arithmetic and nothing else. These run against a live
//! headless REAPER and cover the one thing the unit tests structurally
//! cannot: that the velocities a `Session` resolves are the velocities
//! REAPER ends up holding, addressed by the right note indices.
//!
//! ```sh
//! just reaper daw-test velocity_
//! ```

use daw::rpc::TakeHandle;
use daw::test::reaper_test;
use daw_proto::midi::MidiNoteCreate;
use daw_proto::primitives::{Duration, PositionInSeconds};
use expression_editor_tools::velocity::{
    CurvePreset, Dynamics, Note, Pattern, Pivot, Range, Session,
};

const PPQ: f64 = 960.0;

/// Lay `velocities` out as a run of eighth notes on a fresh track.
async fn take_with(
    ctx: &daw::test::ReaperTestContext,
    name: &str,
    velocities: &[u8],
) -> eyre::Result<TakeHandle> {
    let project = ctx.project().clone();
    let track = project.tracks().add(name, None).await?;
    let item = track
        .items()
        .add(
            PositionInSeconds::from_seconds(0.0),
            Duration::from_seconds(f64::from(velocities.len() as u32) * 0.5),
        )
        .await?;
    let take = item.active_take();
    take.midi()
        .add_notes(
            velocities
                .iter()
                .enumerate()
                .map(|(i, v)| MidiNoteCreate {
                    pitch: 60,
                    velocity: *v,
                    channel: 0,
                    start_ppq: i as f64 * (PPQ / 2.0),
                    length_ppq: PPQ / 4.0,
                })
                .collect(),
        )
        .await?;
    Ok(take)
}

/// Read a take's notes in the shape `midi-tools` works in.
async fn read(take: &TakeHandle) -> eyre::Result<Vec<Note>> {
    Ok(take
        .midi()
        .notes()
        .await?
        .into_iter()
        .map(|n| Note {
            index: n.index,
            velocity: n.velocity,
            selected: n.selected,
        })
        .collect())
}

/// The contract of the whole write path: what `Session::resolve` says,
/// REAPER holds. A drawn ramp is the sharpest test of it — every note
/// gets a different value, so an off-by-one in note indexing shows up as
/// a visibly wrong shape rather than a coincidence.
#[reaper_test(isolated)]
async fn velocity_curve_lands_exactly_as_resolved(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let take = take_with(ctx, "Velocity Curve", &[64; 16]).await?;

    let mut session = Session::new(read(&take).await?);
    session.curve = Some(CurvePreset::Rise.curve());

    let expected: Vec<u8> = session.resolve().iter().map(|n| n.velocity).collect();
    ctx.log(&format!("resolved → {expected:?}"));

    for edit in session.edits() {
        take.midi().set_velocity(edit.index, edit.velocity).await?;
    }

    let actual: Vec<u8> = read(&take).await?.iter().map(|n| n.velocity).collect();
    ctx.log(&format!("REAPER holds {actual:?}"));
    assert_eq!(actual, expected, "REAPER must hold exactly what resolved");
    assert!(
        actual.windows(2).all(|w| w[0] <= w[1]),
        "a rise must arrive as a rise, not shuffled: {actual:?}"
    );
    Ok(())
}

/// Edits are addressed by note index, and a take's notes are not
/// necessarily handed back in the order they were written. This pins that
/// the right note gets the right value by giving every note a distinct
/// starting velocity and a pattern that depends on position.
#[reaper_test(isolated)]
async fn velocity_edits_address_the_right_notes(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let seed: Vec<u8> = (0..8).map(|i| 30 + i * 6).collect();
    let take = take_with(ctx, "Velocity Indexing", &seed).await?;

    let mut session = Session::new(read(&take).await?);
    session.pattern = Pattern::new([100, 20]);
    session.pattern_amount = 1.0;

    for edit in session.edits() {
        take.midi().set_velocity(edit.index, edit.velocity).await?;
    }

    let actual: Vec<u8> = read(&take).await?.iter().map(|n| n.velocity).collect();
    ctx.log(&format!("REAPER holds {actual:?}"));
    assert_eq!(
        actual,
        vec![100, 20, 100, 20, 100, 20, 100, 20],
        "the pattern must land on alternating notes, in take order"
    );
    Ok(())
}

/// Nothing is written for a session sitting at neutral. This is what
/// keeps a slider drag from being hundreds of redundant writes, and it's
/// only observable against a real backend.
#[reaper_test(isolated)]
async fn velocity_neutral_writes_nothing(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let seed: Vec<u8> = (0..12).map(|i| 40 + i * 3).collect();
    let take = take_with(ctx, "Velocity Neutral", &seed).await?;

    let session = Session::new(read(&take).await?);
    assert!(session.edits().is_empty(), "neutral resolves to no edits");

    let actual: Vec<u8> = read(&take).await?.iter().map(|n| n.velocity).collect();
    assert_eq!(
        actual, seed,
        "an untouched session must not disturb the take"
    );
    Ok(())
}

/// Reverting restores the take exactly, even after a commit — the
/// baseline outlives the write.
#[reaper_test(isolated)]
async fn velocity_revert_restores_the_original(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let seed: Vec<u8> = (0..10).map(|i| 50 + i * 7).collect();
    let take = take_with(ctx, "Velocity Revert", &seed).await?;

    let session = {
        let mut s = Session::new(read(&take).await?);
        s.dynamics = Dynamics::new(-0.8, Pivot::Fixed(64));
        s.range = Range::new(20, 110);
        s
    };

    for edit in session.edits() {
        take.midi().set_velocity(edit.index, edit.velocity).await?;
    }
    let squashed: Vec<u8> = read(&take).await?.iter().map(|n| n.velocity).collect();
    assert_ne!(squashed, seed, "compression should have moved something");

    // Revert writes the baseline back over whatever the commit left.
    for note in session.baseline() {
        take.midi().set_velocity(note.index, note.velocity).await?;
    }

    let restored: Vec<u8> = read(&take).await?.iter().map(|n| n.velocity).collect();
    ctx.log(&format!("{seed:?} → {squashed:?} → {restored:?}"));
    assert_eq!(restored, seed, "revert must restore the take exactly");
    Ok(())
}

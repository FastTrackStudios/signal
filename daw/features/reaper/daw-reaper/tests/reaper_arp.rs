//! REAPER integration tests for the arpeggiator write path.
//!
//! The engine is unit-tested against `&[Chord]`. What only a live DAW can
//! prove is the seam: that raw take PPQ round-trips (it does *not* through
//! `add_notes` — see `Midi::add_notes_ppq`), that deleting the source
//! chord takes out the chord and not the arp, and that what comes back
//! out of REAPER is what the engine said.
//!
//! ```sh
//! just reaper daw-test arp_
//! ```

use daw::rpc::TakeHandle;
use daw::test::reaper_test;
use daw_proto::midi::MidiNoteCreate;
use daw_proto::primitives::{Duration, PositionInSeconds};
use expression_editor_tools::arp::{
    Arp, ArpSession, DEFAULT_GAP_PPQ, Direction, PPQ, TimedNote, group_chords,
};

/// Write a chord as simultaneous whole notes on a fresh track.
async fn chord_take(
    ctx: &daw::test::ReaperTestContext,
    name: &str,
    pitches: &[u8],
    bars: f64,
) -> eyre::Result<TakeHandle> {
    let project = ctx.project().clone();
    let track = project.tracks().add(name, None).await?;
    let item = track
        .items()
        .add(
            PositionInSeconds::from_seconds(0.0),
            Duration::from_seconds(bars * 2.0),
        )
        .await?;
    let take = item.active_take();
    take.midi()
        .add_notes(
            pitches
                .iter()
                .map(|pitch| MidiNoteCreate {
                    pitch: *pitch,
                    velocity: 96,
                    channel: 0,
                    // `add_notes` reads this as project quarter-notes.
                    start_ppq: 0.0,
                    length_ppq: PPQ * 4.0 * bars,
                })
                .collect(),
        )
        .await?;
    Ok(take)
}

/// Read a take into the arp's input shape.
async fn read_timed(take: &TakeHandle) -> eyre::Result<(Vec<TimedNote>, Vec<u32>)> {
    let notes = take.midi().notes().await?;
    let indices = notes.iter().map(|n| n.index).collect();
    let timed = notes
        .into_iter()
        .map(|n| TimedNote {
            start_ppq: n.start_ppq,
            length_ppq: n.length_ppq,
            pitch: n.pitch,
            velocity: n.velocity,
        })
        .collect();
    Ok((timed, indices))
}

/// Delete the sources, insert the arp — the commit path, done by hand so
/// the test exercises the same two service calls the sink makes.
async fn commit(take: &TakeHandle, session: &ArpSession) -> eyre::Result<usize> {
    let notes = session.resolve();
    take.midi()
        .delete_notes(session.source_indices().to_vec())
        .await?;
    take.midi()
        .add_notes_ppq(
            notes
                .iter()
                .map(|n| MidiNoteCreate {
                    pitch: n.pitch,
                    velocity: n.velocity,
                    channel: 0,
                    start_ppq: n.start_ppq,
                    length_ppq: n.length_ppq,
                })
                .collect(),
        )
        .await?;
    Ok(notes.len())
}

/// The whole contract: a chord in, the engine's exact arp out.
#[reaper_test(isolated)]
async fn arp_pitches_and_positions_survive_the_round_trip(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let take = chord_take(ctx, "Arp Round Trip", &[60, 64, 67], 1.0).await?;

    let (timed, indices) = read_timed(&take).await?;
    let chords = group_chords(&timed, DEFAULT_GAP_PPQ);
    assert_eq!(chords.len(), 1, "three simultaneous notes are one chord");

    let mut session = ArpSession::new(chords, indices);
    session.arp = Arp::uniform(Direction::Up, PPQ / 4.0);

    let expected = session.resolve();
    ctx.log(&format!("engine → {} notes", expected.len()));
    let written = commit(&take, &session).await?;
    assert_eq!(written, expected.len());

    let mut actual = take.midi().notes().await?;
    actual.sort_by(|a, b| a.start_ppq.partial_cmp(&b.start_ppq).unwrap());

    assert_eq!(actual.len(), expected.len(), "the source chord is gone");

    let got: Vec<u8> = actual.iter().map(|n| n.pitch).collect();
    let want: Vec<u8> = expected.iter().map(|n| n.pitch).collect();
    ctx.log(&format!("REAPER holds {got:?}"));
    assert_eq!(got, want, "pitches must climb exactly as the engine said");

    // Positions are the real test of `add_notes_ppq` — through
    // `add_notes` these would be reinterpreted as quarter-notes and land
    // 960x further out.
    for (a, e) in actual.iter().zip(&expected) {
        assert!(
            (a.start_ppq - e.start_ppq).abs() < 1.0,
            "note at {} should be at {}",
            a.start_ppq,
            e.start_ppq
        );
    }
    Ok(())
}

/// Deleting the source must remove the chord and nothing else. Indices
/// shift as notes are removed, so this is the easiest thing in the whole
/// feature to get subtly wrong.
#[reaper_test(isolated)]
async fn arp_replaces_the_source_chord_exactly(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let take = chord_take(ctx, "Arp Replace", &[48, 52, 55, 59], 1.0).await?;
    assert_eq!(take.midi().notes().await?.len(), 4);

    let (timed, indices) = read_timed(&take).await?;
    let mut session = ArpSession::new(group_chords(&timed, DEFAULT_GAP_PPQ), indices);
    session.arp = Arp::uniform(Direction::Up, PPQ / 2.0);

    commit(&take, &session).await?;

    let after = take.midi().notes().await?;
    ctx.log(&format!("{} notes after commit", after.len()));
    assert_eq!(after.len(), 8, "a bar of eighths, no source remnants");
    // A remnant would be a whole-note-length note; every arp note is an
    // eighth or shorter.
    assert!(
        after.iter().all(|n| n.length_ppq < PPQ),
        "a source note survived: {:?}",
        after.iter().map(|n| n.length_ppq).collect::<Vec<_>>()
    );
    Ok(())
}

/// Two chords in sequence must stay two arpeggios, each inside its own
/// span — the grouping working against real note positions rather than
/// the tidy ones a unit test hands it.
#[reaper_test(isolated)]
async fn arp_keeps_a_progression_in_separate_chords(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Arp Progression", None).await?;
    let item = track
        .items()
        .add(
            PositionInSeconds::from_seconds(0.0),
            Duration::from_seconds(8.0),
        )
        .await?;
    let take = item.active_take();

    // Am for a bar, then F for a bar. `start_ppq` here is quarter-notes.
    let mut creates = Vec::new();
    for (qn, pitches) in [(0.0, [57, 60, 64]), (4.0, [53, 57, 60])] {
        for pitch in pitches {
            creates.push(MidiNoteCreate {
                pitch,
                velocity: 96,
                channel: 0,
                start_ppq: qn,
                length_ppq: PPQ * 4.0,
            });
        }
    }
    take.midi().add_notes(creates).await?;

    let (timed, indices) = read_timed(&take).await?;
    let chords = group_chords(&timed, DEFAULT_GAP_PPQ);
    ctx.log(&format!("grouped into {} chords", chords.len()));
    assert_eq!(chords.len(), 2, "a progression, not one cluster");

    let mut session = ArpSession::new(chords, indices);
    session.arp = Arp::uniform(Direction::Up, PPQ / 4.0);
    commit(&take, &session).await?;

    let after = take.midi().notes().await?;
    let split = after.iter().filter(|n| n.start_ppq < PPQ * 4.0).count();
    ctx.log(&format!("{split} notes in bar 1 of {}", after.len()));
    assert_eq!(split, 16, "the first bar gets its own sixteen sixteenths");
    assert_eq!(after.len(), 32, "and so does the second");
    Ok(())
}

/// Gate must shorten notes without moving them — the upstream
/// double-insert bug showed up exactly here, as a full-length duplicate
/// stacked under every gated note.
#[reaper_test(isolated)]
async fn arp_gate_shortens_without_duplicating(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let take = chord_take(ctx, "Arp Gate", &[60, 64, 67], 1.0).await?;

    let (timed, indices) = read_timed(&take).await?;
    let mut session = ArpSession::new(group_chords(&timed, DEFAULT_GAP_PPQ), indices);
    session.arp = Arp::uniform(Direction::Up, PPQ / 4.0);
    session.set_gate(0.5);

    commit(&take, &session).await?;

    let after = take.midi().notes().await?;
    assert_eq!(after.len(), 16, "sixteen notes, not thirty-two");
    for note in &after {
        assert!(
            (note.length_ppq - PPQ / 8.0).abs() < 1.0,
            "gated note should be a 32nd, got {}",
            note.length_ppq
        );
    }
    Ok(())
}

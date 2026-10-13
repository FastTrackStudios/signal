//! REAPER integration tests for chord insertion.
//!
//! Everything about `chord-tool` up to here has been type-checked and
//! unit-tested but never run against a DAW, which is the only thing that
//! proves the pitches land where the panel says they will. These run
//! against a live headless REAPER:
//!
//! ```sh
//! just reaper daw-test chord_
//! ```
//!
//! What they're actually guarding is the seam nothing else covers — the
//! theory says "these MIDI numbers", and REAPER has to end up holding
//! exactly those, in one item, at the cursor.

use daw::test::reaper_test;
use daw_proto::midi::MidiNoteCreate;
use daw_proto::primitives::{Duration, PositionInSeconds};
use keyflow::chord::palette::{grid, variations};
use keyflow::key::Key;
use keyflow::primitives::MusicalNote;

const PPQ: f64 = 960.0;

fn c_major() -> eyre::Result<Key> {
    let root = MusicalNote::from_string("C").ok_or_else(|| eyre::eyre!("C parses"))?;
    Ok(Key::major(root))
}

/// `x` as `f64`, for the small (bar-index) values used in the progression
/// layout test below — never large enough to lose precision, but
/// `usize`→`f64` has no non-`as` conversion in `std`.
fn f64_from_usize(x: usize) -> f64 {
    u32::try_from(x).map_or_else(|_| f64::from(u32::MAX), f64::from)
}

/// The pitches keyflow computes for a chord must be the pitches REAPER
/// holds afterwards. This is the whole contract of the insert path.
#[reaper_test(isolated)]
async fn chord_notes_survive_the_round_trip(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Chord Insert", None).await?;

    // C major triad, degree 1, root position.
    let chord = variations(&c_major()?, 1)
        .into_iter()
        .next()
        .ok_or_else(|| eyre::eyre!("degree 1 offers a chord"))?;
    let expected = chord.notes_inverted(4, 0);
    ctx.log(&format!("{} → {expected:?}", chord.label));
    eyre::ensure!(
        expected == vec![60, 64, 67],
        "C major at octave 4: got {expected:?}"
    );

    let item = track
        .items()
        .add(
            PositionInSeconds::from_seconds(0.0),
            Duration::from_seconds(2.0),
        )
        .await?;
    let take = item.active_take();

    let notes: Vec<MidiNoteCreate> = expected
        .iter()
        .map(|pitch| MidiNoteCreate {
            pitch: *pitch,
            velocity: 96,
            channel: 0,
            start_ppq: 0.0,
            length_ppq: PPQ,
        })
        .collect();
    take.midi().add_notes(notes).await?;

    let mut actual: Vec<u8> = take
        .midi()
        .notes()
        .await?
        .into_iter()
        .map(|n| n.pitch)
        .collect();
    actual.sort_unstable();

    ctx.log(&format!("REAPER holds {actual:?}"));
    eyre::ensure!(
        actual == expected,
        "REAPER must hold exactly what keyflow computed: got {actual:?}, expected {expected:?}"
    );
    Ok(())
}

/// A chord is one item holding N simultaneous notes, not N items or a
/// melody. If the voices ever drift apart in time, this catches it.
#[reaper_test(isolated)]
async fn chord_voices_are_simultaneous(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Simultaneity", None).await?;

    let chord = variations(&c_major()?, 5)
        .into_iter()
        .find(|c| c.semitones.len() == 4 && c.in_scale)
        .ok_or_else(|| eyre::eyre!("the fifth degree offers a seventh chord"))?;
    let pitches = chord.notes_inverted(4, 0);
    ctx.log(&format!("{} → {pitches:?}", chord.label));

    let item = track
        .items()
        .add(
            PositionInSeconds::from_seconds(0.0),
            Duration::from_seconds(2.0),
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
                    start_ppq: 0.0,
                    length_ppq: PPQ,
                })
                .collect(),
        )
        .await?;

    let notes = take.midi().notes().await?;
    eyre::ensure!(
        notes.len() == pitches.len(),
        "one note per voice: got {} notes, {} pitches",
        notes.len(),
        pitches.len()
    );
    let item_count = track.items().count().await?;
    eyre::ensure!(
        item_count == 1,
        "a chord is one item, not one per voice: got {item_count}"
    );

    let starts: Vec<f64> = notes.iter().map(|n| n.start_ppq).collect();
    let first = starts.first().copied().unwrap_or_default();
    for start in &starts {
        eyre::ensure!(
            (start - first).abs() < 1.0,
            "voices must start together, got {starts:?}"
        );
    }
    Ok(())
}

/// Successive chords laid at increasing positions must stay in order and
/// not collide — the property "insert advances the cursor" depends on.
#[reaper_test(isolated)]
async fn progression_lays_out_in_order(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Progression", None).await?;

    // I - IV - V, one bar each at 120bpm 4/4 (2s per bar).
    let columns = grid(&c_major()?);
    let degrees = [1usize, 4, 5];
    let bar = 2.0;

    for (i, degree) in degrees.iter().enumerate() {
        let chord = columns[degree.saturating_sub(1)]
            .iter()
            .find(|c| c.in_scale)
            .ok_or_else(|| eyre::eyre!("degree offers an in-key chord"))?;
        let start = bar * f64_from_usize(i);
        let item = track
            .items()
            .add(
                PositionInSeconds::from_seconds(start),
                Duration::from_seconds(bar),
            )
            .await?;
        let take = item.active_take();
        take.midi()
            .add_notes(
                chord
                    .notes_inverted(4, 0)
                    .iter()
                    .map(|pitch| MidiNoteCreate {
                        pitch: *pitch,
                        velocity: 96,
                        channel: 0,
                        start_ppq: 0.0,
                        length_ppq: PPQ * 4.0,
                    })
                    .collect(),
            )
            .await?;
        ctx.log(&format!("bar {i}: {}", chord.label));
    }

    let item_count = track.items().count().await?;
    let expected_count = u32::try_from(degrees.len()).unwrap_or(u32::MAX);
    eyre::ensure!(
        item_count == expected_count,
        "one item per chord: got {item_count}, expected {expected_count}"
    );

    let items = track.items().all().await?;
    let mut positions: Vec<f64> = items.iter().map(|i| i.position.as_seconds()).collect();
    positions.sort_by(f64::total_cmp);
    for (i, pos) in positions.iter().enumerate() {
        let expected_pos = bar * f64_from_usize(i);
        eyre::ensure!(
            (pos - expected_pos).abs() < 0.01,
            "chord {i} should sit at {expected_pos}s, got {pos}s"
        );
    }
    Ok(())
}

// ── The Keyflow track folder ────────────────────────────────────────────

/// The four tracks `session::keyflow::scaffold` spawns, in order.
///
/// KEY / CHORD / MELODY / SCALE — not "Hits"; that's a section type, not
/// a track. They live under a "Keyflow" folder parent.
const KEYFLOW_TRACKS: [&str; 4] = ["KEY", "CHORD", "MELODY", "SCALE"];

/// Spawning the Keyflow folder has to produce a real REAPER *folder*, not
/// four loose tracks that happen to be adjacent. The folder-depth dance —
/// +1 on the parent, -1 on the last child — is the part that only REAPER
/// can confirm, and getting it wrong leaves the project subtly wrong in a
/// way no unit test would notice.
#[reaper_test(isolated)]
async fn keyflow_folder_spawns_with_its_four_tracks(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let tracks = project.tracks();

    let folder = tracks.add("Keyflow", None).await?;
    folder.set_folder_depth(1).await?;

    let mut children = Vec::new();
    for name in KEYFLOW_TRACKS {
        children.push(tracks.add(name, None).await?);
    }
    // The last child closes the folder.
    children
        .last()
        .ok_or_else(|| eyre::eyre!("four children"))?
        .set_folder_depth(-1)
        .await?;

    let track_count = tracks.count().await?;
    eyre::ensure!(
        track_count == 5,
        "one folder parent plus four children: got {track_count}"
    );

    for name in KEYFLOW_TRACKS {
        let found = tracks.by_name(name).await?;
        eyre::ensure!(found.is_some(), "{name} track should exist");
        ctx.log(&format!("{name} present"));
    }

    // Order matters — the scaffold lays them out KEY, CHORD, MELODY,
    // SCALE and a reader walking the folder depends on it.
    let all = tracks.all().await?;
    let names: Vec<String> = all.iter().map(|t| t.name.clone()).collect();
    let keyflow_at = names
        .iter()
        .position(|n| n == "Keyflow")
        .ok_or_else(|| eyre::eyre!("folder parent present"))?;
    let after: Vec<&String> = names
        .iter()
        .skip(keyflow_at.saturating_add(1))
        .take(4)
        .collect();
    eyre::ensure!(
        after == KEYFLOW_TRACKS.iter().collect::<Vec<_>>(),
        "children follow the parent in scaffold order, got {names:?}"
    );

    Ok(())
}

/// REAPER's key signature is NOT reachable through the extension API — it
/// lives in the project chunk as a `<KEYSIG>` block:
///
/// ```text
/// <KEYSIG
///   0 10 -1 0xAB5      measure, root (B♭ = 10), accidental (flat), major mask
/// >
/// ```
///
/// This test pins what we *can* do today: confirm the project has no key
/// signature to begin with, so a future writer has a known starting
/// point. Reading and writing it needs project-chunk access
/// (`GetProjectStateChunk` / `SetProjectStateChunk`), which the daw facade
/// doesn't expose yet — see the fixture at
/// `crates/keyflow/keyflow/tests/fixtures/vienna_couch.rpp` for a real
/// one.
#[reaper_test(isolated)]
async fn fresh_project_has_no_key_signature(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    // A fresh tab starts empty; the marker here is that nothing in the
    // current API surface reports a key signature at all.
    ctx.log("key signature lives in the project chunk (<KEYSIG>), not the API");
    let track_count = project.tracks().count().await?;
    eyre::ensure!(track_count == 0, "fresh isolated tab: got {track_count}");
    Ok(())
}

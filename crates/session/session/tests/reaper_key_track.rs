//! REAPER integration tests for the Key track.
//!
//! The unit tests prove labels round-trip as strings. These prove the
//! thing that actually matters and that only REAPER can answer: that a
//! key change written as an item label survives the trip through a real
//! project and reads back as the same key.
//!
//! ```sh
//! just reaper daw-test key_track
//! just reaper daw-test --gui key_track_round_trips   # watch it
//! ```

use daw::test::reaper_test;
use daw_proto::primitives::{Duration, PositionInSeconds};
use session::key::{KEY_TRACK, format_key, parse_key};

use keyflow::key::Key;
use keyflow::primitives::MusicalNote;

fn key_of(root: &str, major: bool) -> eyre::Result<Key> {
    let note =
        MusicalNote::from_string(root).ok_or_else(|| eyre::eyre!("invalid note: {}", root))?;
    if major {
        Ok(Key::major(note))
    } else {
        Ok(Key::minor(note))
    }
}

/// A key written into an item's label comes back as the same key. This is
/// the whole premise of storing key changes in the project rather than in
/// a side-car: if REAPER mangles the text, the design fails.
#[reaper_test(isolated)]
async fn key_track_round_trips_through_reaper(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add(KEY_TRACK, None).await?;

    let key = key_of("Eb", true)?;
    let item = track
        .items()
        .add(
            PositionInSeconds::from_seconds(0.0),
            Duration::from_seconds(2.0),
        )
        .await?;
    item.set_label(&format_key(&key)).await?;

    let read_back = item.label().await?.expect("the label should persist");
    ctx.log(&format!("label survived as {read_back:?}"));
    if read_back != "Eb major" {
        return Err(eyre::eyre!("expected 'Eb major', got '{read_back}'"));
    }
    if parse_key(&read_back) != Some(key) {
        return Err(eyre::eyre!("same key after the trip"));
    }
    Ok(())
}

/// Several changes, each on its own item, stay distinct and keep their
/// positions — the property "the key at bar N" depends on.
#[reaper_test(isolated)]
async fn key_changes_keep_their_positions(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add(KEY_TRACK, None).await?;

    let plan = [(0.0, "C", true), (4.0, "A", false), (8.0, "F#", false)];
    for (at, root, major) in plan {
        let item = track
            .items()
            .add(
                PositionInSeconds::from_seconds(at),
                Duration::from_seconds(2.0),
            )
            .await?;
        let key = key_of(root, major)?;
        item.set_label(&format_key(&key)).await?;
    }

    let items = track.items().all().await?;
    if items.len() != 3 {
        return Err(eyre::eyre!(
            "one item per change: expected 3, got {}",
            items.len()
        ));
    }

    let mut found: Vec<(f64, String)> = Vec::new();
    for info in items {
        let handle = track
            .items()
            .by_guid(&info.guid)
            .await?
            .expect("the item we just made");
        let label = handle.label().await?.unwrap_or_default();
        found.push((info.position.as_seconds(), label));
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));

    ctx.log(&format!("{found:?}"));
    if found.get(0) != Some(&(0.0, "C major".to_string())) {
        return Err(eyre::eyre!(
            "expected found[0] = (0.0, \"C major\"), got {:?}",
            found.get(0)
        ));
    }
    if found.get(1) != Some(&(4.0, "A minor".to_string())) {
        return Err(eyre::eyre!(
            "expected found[1] = (4.0, \"A minor\"), got {:?}",
            found.get(1)
        ));
    }
    if found.get(2) != Some(&(8.0, "F# minor".to_string())) {
        return Err(eyre::eyre!(
            "expected found[2] = (8.0, \"F# minor\"), got {:?}",
            found.get(2)
        ));
    }
    Ok(())
}

/// Accidentals are the part most likely to be mangled by a round trip
/// through a text field, and the part that matters most — Db major and
/// C# major are different keys.
#[reaper_test(isolated)]
async fn accidentals_survive_the_round_trip(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add(KEY_TRACK, None).await?;

    for (i, root) in ["C#", "Db", "F#", "Gb", "Bb"].iter().enumerate() {
        let key = key_of(root, true)?;
        let i_u32 = u32::try_from(i).unwrap_or(0);
        let item = track
            .items()
            .add(
                PositionInSeconds::from_seconds(f64::from(i_u32) * 2.0),
                Duration::from_seconds(1.0),
            )
            .await?;
        item.set_label(&format_key(&key)).await?;

        let back = item.label().await?.expect("label");
        if parse_key(&back) != Some(key.clone()) {
            return Err(eyre::eyre!(
                "{root} major came back as {back:?}, expected {:?}",
                Some(key)
            ));
        }
        ctx.log(&format!("{root} major ok"));
    }
    Ok(())
}

// ── Baking to <KEYSIG> ──────────────────────────────────────────────────

/// Baking turns key changes at *times* into `<KEYSIG>` entries at
/// *measures*, and that conversion runs through the project's real tempo
/// map. Everything else about the bake is covered by unit tests against a
/// REAPER-written fixture (`dawfile_reaper::keysig`); this is the part
/// only a live project can answer.
///
/// At 120bpm 4/4 a bar is two seconds. The catch — which this test found
/// — is that `time_to_musical` returns **1-based** measures (0s is
/// measure 1) while `<KEYSIG>` counts from **0**. Baking without that
/// subtraction puts every key signature exactly one bar late, in a way
/// nothing but a live project would reveal.
#[reaper_test(isolated)]
async fn key_positions_convert_to_the_right_measures(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    project.transport().set_tempo(120.0).await?;

    let bar = 2.0; // 120bpm, 4/4
    for (i, seconds) in [0.0, bar * 4.0, bar * 8.0].iter().enumerate() {
        let (measure, beat, _) = project.tempo_map().time_to_musical(*seconds).await?;
        ctx.log(&format!(
            "{seconds}s -> REAPER measure {measure}, beat {beat}"
        ));
        let measure_usize = usize::try_from(measure).unwrap_or(0);
        let expected_measure = i.saturating_mul(4).saturating_add(1);
        if measure_usize != expected_measure {
            return Err(eyre::eyre!(
                "REAPER numbers measures from 1: expected {}, got {}",
                expected_measure,
                measure_usize
            ));
        }
        let keysig_measure_val = i.saturating_mul(4);
        let keysig_measure_u32 = u32::try_from(keysig_measure_val).unwrap_or(u32::MAX);
        if session::key::keysig_measure(measure) != keysig_measure_u32 {
            return Err(eyre::eyre!(
                "KEYSIG numbers them from 0: expected {}, got {}",
                keysig_measure_u32,
                session::key::keysig_measure(measure)
            ));
        }
        if beat != 1 {
            return Err(eyre::eyre!(
                "a downbeat — REAPER numbers beats from 1 as well: expected 1, got {}",
                beat
            ));
        }
    }
    Ok(())
}

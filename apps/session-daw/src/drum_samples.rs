//! Drum samples under the recorded kit: signal's sampled kit, played from
//! the hits the triggers caught.
//!
//! Session's hit markers say where the kit's hits are (`crate::hits`,
//! found once from the triggers and mics); signal plays a sampled kit
//! from notes. This joins them, blended under the recording:
//!
//! - each piece's hits written as notes on its **Sample** track (the
//!   kit's General MIDI note for the piece, the velocity from the hit's
//!   level), and an `FTS Kit Mic` there playing every mic of that piece,
//!   split: its **close** mics on the track's own output, its
//!   **overhead** mics on 3/4 and its **room** mics on 5/6;
//! - the Sample track sending its 3/4 into the recorded overheads and its
//!   5/6 into the rooms, so the sample's ambience is processed and faded
//!   with theirs — sends, not tracks of their own.
//!
//! One instrument plays all of a piece's mics from the same notes, so each
//! hit is the same sample on every one.

use daw::service::TempoMap as _;
use daw::service::{FxChainContext, ItemRef, Items as _, ProjectContext, TrackRef, Tracks};
use daw_proto::{Midi as _, MidiNoteCreate, MidiTakeLocation, Routing as _, TakeRef};

/// The kit's note for a piece of the session's kit, by its name: General
/// MIDI's kick and snare, the toms high to low onto the kit's rack then
/// floor toms (`Tom 1` the smallest).
#[must_use]
pub fn note_for(piece: &str) -> Option<u8> {
    let name = piece.trim().to_ascii_lowercase();
    if name.contains("kick") {
        return Some(36);
    }
    if name.contains("snare") {
        return Some(38);
    }
    let tom = name.strip_prefix("tom")?.trim().parse::<u8>().ok()?;
    [45, 47, 41, 43]
        .get(usize::from(tom.checked_sub(1)?))
        .copied()
}

/// A hit's velocity from its level against its piece's loudest (dB, 0
/// the loudest): the loudest at 127, half the velocity every 12 dB down.
#[must_use]
pub fn velocity(relative_db: f64) -> u8 {
    let v = (127.0 * 10_f64.powf(relative_db.min(0.0) / 40.0)).round();
    num_traits::ToPrimitive::to_u8(&v.clamp(1.0, 127.0)).unwrap_or(1)
}

/// How long each written note is, quarter notes: a drum's sample plays
/// out whatever the length.
const NOTE_QN: f64 = 0.25;

/// A project file's MIDI resolution, ticks a quarter note: a note is
/// written on one, so the saved song plays it where the live one did (off
/// a tick, a hit moved by up to half of one, ~0.3 ms at 100 bpm, on save).
const TICKS_PER_QN: f64 = 960.0;

/// What setting the samples up did, or would.
#[derive(Debug, Default)]
pub struct Report {
    /// One line a track.
    pub tracks: Vec<String>,
    pub notes: usize,
}

/// One piece's notes: `(time, note, velocity)`.
type Notes = Vec<(f64, u8, u8)>;

/// Set the samples up on the kit; `dry`, only say.
///
/// # Errors
///
/// No song open, or the engine refusing a write.
pub fn set_up(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    preset: &std::path::Path,
    dry: bool,
) -> Result<Report, String> {
    let kit = played(daw, project);
    if kit.is_empty() {
        return Err("no hit markers: find the hits first (HITS_DETECT)".to_owned());
    }
    let triggers = crate::drum_reference::pieces(daw, project);
    let tracks = Tracks::all(daw, project.clone());
    let overheads = bus_track(&tracks, &["ohs", "overheads", "oh"]).cloned();
    let rooms = bus_track(&tracks, &["rooms", "room"]).cloned();
    let mut report = Report::default();
    for (name, hits) in &kit {
        let Some(note) = note_for(name) else {
            report
                .tracks
                .push(format!("{name}: no note in the kit for it"));
            continue;
        };
        let Some(piece) = signal_fx::kit::piece_for_note(preset, note) else {
            report
                .tracks
                .push(format!("{name}: the kit has nothing on note {note}"));
            continue;
        };
        let Some(sample) = triggers
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, trigger)| sample_track(&tracks, trigger))
        else {
            report
                .tracks
                .push(format!("{name}: no Sample track beside its Trig"));
            continue;
        };
        let notes: Notes = hits
            .iter()
            .map(|&(time, relative_db)| (time, note, velocity(relative_db)))
            .collect();
        report.notes = report.notes.saturating_add(notes.len());
        report.tracks.push(format!(
            "{}: {} notes on {note} ({piece}), its close mics out, overheads on 3/4, rooms on 5/6",
            sample.name,
            notes.len(),
        ));
        if !dry {
            write_notes(daw, project, &sample.guid, &notes)?;
            let state = serde_json::json!({
                "preset": preset,
                "pieces": [piece],
                "mics": ["close", "overhead", "room"],
                "split": true,
            });
            kit_mic(daw, project, &sample.guid, &state)?;
            for (from, bus) in [(2, &overheads), (4, &rooms)] {
                if let Some(bus) = bus {
                    send_from(daw, project, &sample.guid, &bus.guid, from)?;
                }
            }
        }
    }
    // The separate ambience tracks an earlier set-up made: their mics are
    // now the Sample tracks' own, sent from 3/4 and 5/6.
    for t in &tracks {
        if ["oh sample", "room sample"].contains(&t.name.trim().to_ascii_lowercase().as_str()) {
            report
                .tracks
                .push(format!("{}: removed — sent from each Sample track", t.name));
            if !dry {
                Tracks::remove(daw, project.clone(), TrackRef::Guid(t.guid.clone()))
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    for (bus, pair, beside) in [
        (&overheads, "3/4", &["ohs", "overheads", "oh"][..]),
        (&rooms, "5/6", &["rooms", "room"][..]),
    ] {
        report.tracks.push(bus.as_ref().map_or_else(
            || format!("no {} track to send {pair} into", beside.join(" / ")),
            |bus| format!("each Sample's {pair} sent into {}", bus.name),
        ));
    }
    if !dry {
        crate::studio::request_resync();
        crate::actions::list().changed("drums.samples", "the kit's samples were set up");
    }
    Ok(report)
}

/// Each piece's hits where they play now, `(time, dB against the piece's
/// loudest)`, earliest first: every hit marker but a bleed nobody checked
/// — a ghost note too, at its own quiet velocity.
fn played(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
) -> Vec<(String, Vec<(f64, f64)>)> {
    let mut by_piece: Vec<(String, Vec<(f64, f64)>)> = Vec::new();
    for p in crate::hits::playback(daw, project) {
        if p.mark.bleed && p.mark.origin == daw_proto::MarkOrigin::Auto {
            continue;
        }
        let at = by_piece
            .iter()
            .position(|(n, _)| *n == p.piece)
            .unwrap_or_else(|| {
                by_piece.push((p.piece.clone(), Vec::new()));
                by_piece.len().saturating_sub(1)
            });
        if let Some((_, hits)) = by_piece.get_mut(at) {
            hits.push((p.time, p.peak_db));
        }
    }
    for (_, hits) in &mut by_piece {
        let loudest = hits.iter().map(|h| h.1).fold(f64::NEG_INFINITY, f64::max);
        for h in hits.iter_mut() {
            h.1 -= loudest;
        }
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
    }
    by_piece
}

/// Write every Sample track's notes where its piece's hits play now — the
/// samples brought back onto the recording, whatever moved it. Returns
/// how many tracks were written.
///
/// # Errors
///
/// The engine refusing a write.
pub fn resync(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
) -> Result<usize, String> {
    follow(daw, project, |t| t)
}

/// Move the samples with an edit about to be made.
///
/// Every Sample track already set up (holding its notes) is written again
/// from the hit markers, each hit where `moved` puts it; the rest are
/// left. Returns how many tracks were written.
///
/// # Errors
///
/// The engine refusing a write.
pub fn follow(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    moved: impl Fn(f64) -> f64,
) -> Result<usize, String> {
    let tracks = Tracks::all(daw, project.clone());
    let triggers = crate::drum_reference::pieces(daw, project);
    let mut written = 0_usize;
    for (name, hits) in played(daw, project) {
        let Some(note) = note_for(&name) else {
            continue;
        };
        let Some(sample) = triggers
            .iter()
            .find(|(n, _)| *n == name)
            .and_then(|(_, trigger)| sample_track(&tracks, trigger))
        else {
            continue;
        };
        if daw
            .get_items(project.clone(), TrackRef::Guid(sample.guid.clone()))
            .is_empty()
        {
            continue;
        }
        let mut notes: Notes = hits
            .iter()
            .map(|&(time, relative_db)| (moved(time), note, velocity(relative_db)))
            .collect();
        notes.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Already where they go (an undo that moved nothing of the kit):
        // left as they are, not written again.
        if already(daw, project, &sample.guid, &notes) {
            continue;
        }
        write_notes(daw, project, &sample.guid, &notes)?;
        written = written.saturating_add(1);
    }
    Ok(written)
}

/// How near a written note must be to where it goes to be left, seconds:
/// a MIDI tick at any tempo a kit is played at.
const SAME_NOTE: f64 = 0.0005;

/// Whether `track` already plays `notes`: as many, each at its time, pitch
/// and velocity.
fn already(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    notes: &[(f64, u8, u8)],
) -> bool {
    let have = crate::drum_reference::notes(daw, project, track);
    have.len() == notes.len()
        && have.iter().zip(notes).all(|(h, &(time, pitch, velocity))| {
            (h.time - time).abs() < SAME_NOTE && h.pitch == pitch && h.velocity == velocity
        })
}

/// The recorded mics' track a sample bus sends into: the first of
/// `names` (in order of preference) that a track of the kit is called —
/// never a reverb's return of the same name (`Kick/Verb/Room`).
fn bus_track<'a>(tracks: &'a [daw_proto::Track], names: &[&str]) -> Option<&'a daw_proto::Track> {
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let in_a_verb = |t: &daw_proto::Track| {
        crate::gain_stage::path_of(t, &by_guid)
            .iter()
            .any(|p| p.trim().eq_ignore_ascii_case("verb"))
    };
    names.iter().find_map(|name| {
        tracks
            .iter()
            .find(|t| !t.is_folder && t.name.trim().eq_ignore_ascii_case(name) && !in_a_verb(t))
    })
}

/// The Sample track beside a piece's trigger: a sibling named `Sample`
/// (`Sample`, `T1 Sample`).
fn sample_track<'a>(tracks: &'a [daw_proto::Track], trigger: &str) -> Option<&'a daw_proto::Track> {
    let parent = tracks
        .iter()
        .find(|t| t.guid == trigger)?
        .parent_guid
        .clone();
    tracks.iter().find(|t| {
        t.parent_guid == parent
            && !t.is_folder
            && t.name.trim().to_ascii_lowercase().ends_with("sample")
    })
}

/// `notes` as the one MIDI item on `track`: what was there before is
/// replaced (a Sample track holds the notes this writes, nothing else).
fn write_notes(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    notes: &[(f64, u8, u8)],
) -> Result<(), String> {
    for item in daw.get_items(project.clone(), TrackRef::Guid(track.to_owned())) {
        daw.delete_item(project.clone(), ItemRef::Guid(item.guid))
            .map_err(|e| e.to_string())?;
    }
    let (Some(first), Some(last)) = (notes.first(), notes.last()) else {
        return Ok(());
    };
    let start = (first.0 - 1.0).max(0.0);
    let location: MidiTakeLocation = daw
        .create_midi_item(
            project.clone(),
            TrackRef::Guid(track.to_owned()),
            start,
            last.0 + 5.0,
        )
        .ok_or("the MIDI item would not make")?;
    let tempos: Vec<(f64, f64)> = daw
        .get_tempo_points(project.clone())
        .into_iter()
        .filter_map(|p| Some((p.position.time?.as_seconds(), p.bpm)))
        .collect();
    let bpm = daw.get_default_tempo(project.clone());
    let created = placed(notes, |time| {
        crate::drum_reference::qn_between(start, time, &tempos, bpm)
    });
    let _ = daw.add_notes_ppq(
        MidiTakeLocation::new(location.project, location.item, TakeRef::Active),
        created,
    );
    Ok(())
}

/// `notes` as MIDI notes, at `qn(time)` quarter notes into the item: each
/// on a tick ([`TICKS_PER_QN`]) and ending a tick before the next of its
/// pitch — overlapping notes of one pitch are not two notes once written
/// as ons and offs (they read back as one cut short and one of no
/// length), and a drum's sample plays out whatever the note's length.
fn placed(notes: &[(f64, u8, u8)], qn: impl Fn(f64) -> f64) -> Vec<MidiNoteCreate> {
    // Whole ticks, divided once — as a file's reader does — so a note
    // reads back as the very value it was written with.
    let ticks: Vec<(f64, u8, u8)> = notes
        .iter()
        .map(|&(time, pitch, vel)| ((qn(time) * TICKS_PER_QN).round(), pitch, vel))
        .collect();
    let longest = NOTE_QN * TICKS_PER_QN;
    ticks
        .iter()
        .enumerate()
        .map(|(i, &(start, pitch, vel))| {
            let next = ticks
                .iter()
                .skip(i.saturating_add(1))
                .find(|n| n.1 == pitch && n.0 > start)
                .map_or(f64::INFINITY, |n| n.0);
            let length = longest.min(next - start - 1.0).max(1.0);
            MidiNoteCreate::new(pitch, vel, start / TICKS_PER_QN, length / TICKS_PER_QN)
        })
        .collect()
}

/// A send from `track`'s channels `from`+1/`from`+2 (2: its 3/4, 4: its
/// 5/6) into `bus`, post-fader — the one there, or a new one.
fn send_from(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    bus: &str,
    from: u32,
) -> Result<(), String> {
    let source = TrackRef::Guid(track.to_owned());
    let there = daw
        .sends(project.clone(), source.clone())
        .into_iter()
        .find(|s| {
            s.dest_track_guid.as_deref() == Some(bus) && s.source_channels.start_channel == from
        });
    let index = match there {
        Some(send) => send.index,
        None => daw
            .add_send(
                project.clone(),
                source.clone(),
                TrackRef::Guid(bus.to_owned()),
            )
            .ok_or("the send would not add")?,
    };
    daw.set_source_channels(
        project.clone(),
        daw_proto::RouteLocation {
            track: source,
            route_type: daw_proto::RouteType::Send,
            route: daw_proto::RouteRef::Index(index),
        },
        from,
        2,
    )
    .map_err(|e| e.to_string())
}

/// An `FTS Kit Mic` on `track` (the one there, or a new one) playing what
/// `state` says.
fn kit_mic(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    state: &serde_json::Value,
) -> Result<(), String> {
    let chain = FxChainContext::Track(track.to_owned());
    let fx = match daw::service::Effects::list(daw, project.clone(), chain.clone())
        .into_iter()
        .find(|f| f.name.eq_ignore_ascii_case(signal_fx::KIT_MIC))
    {
        Some(f) => f.guid,
        None => daw::service::Effects::add(daw, project.clone(), chain, signal_fx::KIT_MIC)
            .ok_or("the kit mic would not add")?,
    };
    daw.apply_plugin_state(&fx, state.to_string().as_bytes())
        .map_err(|e| format!("the kit mic would not load: {e}"))
}

#[cfg(test)]
mod tests {

    /// Notes land on ticks, and one ends a tick before the next of its
    /// pitch — never under it — while another pitch is left alone.
    #[test]
    fn notes_are_on_ticks_and_never_overlap_their_pitch() {
        let notes = [(0.0, 36, 100), (0.1, 36, 90), (0.1, 38, 80), (1.0, 36, 70)];
        let placed = placed(&notes, |t| t + 0.000_31);
        let tick = 1.0 / TICKS_PER_QN;
        for n in &placed {
            let ticks = n.start_ppq / tick;
            assert!(
                (ticks - ticks.round()).abs() < 1e-6,
                "on a tick: {}",
                n.start_ppq
            );
        }
        let first = &placed[0];
        assert!((first.start_ppq + first.length_ppq - (placed[1].start_ppq - tick)).abs() < 1e-9);
        assert!(
            (placed[2].length_ppq - NOTE_QN).abs() < 1e-9,
            "the snare keeps its length"
        );
        assert!(
            (placed[3].length_ppq - NOTE_QN).abs() < 1e-9,
            "room to ring"
        );
    }

    use super::*;

    #[test]
    fn pieces_take_the_kits_notes() {
        assert_eq!(note_for("Kick"), Some(36));
        assert_eq!(note_for("Snare"), Some(38));
        assert_eq!(note_for("Tom 1"), Some(45));
        assert_eq!(note_for("tom 4"), Some(43));
        assert_eq!(note_for("Tom 5"), None);
        assert_eq!(note_for("Hi Hat"), None);
    }

    #[test]
    fn velocity_follows_the_level() {
        assert_eq!(velocity(0.0), 127);
        assert_eq!(velocity(3.0), 127, "never past the loudest");
        assert_eq!(velocity(-12.0), 64);
        assert_eq!(velocity(-24.0), 32);
        assert_eq!(velocity(-200.0), 1);
    }
}

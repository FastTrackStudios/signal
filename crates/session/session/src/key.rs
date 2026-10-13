//! The Key track — key changes as items you can see and move.
//!
//! REAPER has no API for key signatures (the project's `<KEYSIG>` block
//! is the only representation and nothing live reads or writes it — see
//! `dawfile_reaper::keysig`). So FTS keeps its own, and keeps it
//! somewhere better than a hidden blob: a `KEY` track whose items carry
//! the key in their label.
//!
//! That's the `AikyaLabs` Simple `ChordTrack` pattern. REAPER draws an item's
//! `P_NOTES` as its on-screen text, so a key change is visible in the
//! arrange view, draggable to a different bar, editable by hand, saved
//! with the project, and readable back without a side-car store. An
//! `ExtState` blob would be none of those things.
//!
//! What this does *not* do is drive REAPER's key snap, which reads
//! `<KEYSIG>`. [`crate::key::bake`] is the separate, explicit step for
//! that.

use daw::service::{
    Duration, ItemRef, Items, PositionInSeconds, ProjectContext, Projects, TempoMap, TrackRef,
    Tracks,
};
use daw_proto::{DawError, DawResult};
use keyflow::key::Key;
use keyflow::key::scale::ScaleMode;
use keyflow::primitives::MusicalNote;

/// The track key changes live on.
pub const KEY_TRACK: &str = "KEY";

/// How long a key-change item is drawn, in seconds. Purely visual — the
/// change applies until the next one — but a zero-length item can't be
/// grabbed, so it needs *some* width.
const MARKER_SECONDS: f64 = 2.0;

/// A dusky violet, distinct from the section and chord colours.
const KEY_COLOR: u32 = 0x007C_3AED;

/// A key change at a point on the timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyChange {
    pub seconds: f64,
    pub key: Key,
}

/// Render a key the way it's stored in an item label: `"Eb major"`.
///
/// Round-trips through [`parse_key`]. Deliberately the plainest thing
/// that reads correctly to a human, since the whole point of storing it
/// in the label is that a person can read and edit it.
#[must_use]
pub fn format_key(key: &Key) -> String {
    // Ionian and Aeolian get their common names. Musicians write "C
    // major", not "C ionian", and this string is meant to be read and
    // typed by a person.
    let mode = match key.mode.name() {
        "Ionian" => "major".to_string(),
        "Aeolian" => "minor".to_string(),
        other => other.to_lowercase(),
    };
    format!("{} {mode}", normalize_root(&key.root.name))
}

/// `"eb"` → `"Eb"`, `"f#"` → `"F#"`.
///
/// `MusicalNote::from_string` keeps whatever casing it was handed, so two
/// keys parsed from `"c major"` and `"C major"` compare unequal without
/// this. The label is hand-editable, so both spellings will happen.
fn normalize_root(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        let rest: String = chars.collect();
        format!(
            "{}{}",
            first.to_ascii_uppercase(),
            rest.to_ascii_lowercase()
        )
    })
}

/// Parse an item label back into a key. `None` for anything that isn't
/// one, so unrelated items on the track are ignored rather than guessed
/// at.
pub fn parse_key(label: &str) -> Option<Key> {
    let text = label.trim();
    let (root_text, mode_text) = text.split_once(char::is_whitespace)?;
    let root = MusicalNote::from_string(&normalize_root(root_text.trim()))?;
    let mode = match mode_text.trim().to_ascii_lowercase().as_str() {
        "major" | "ionian" => ScaleMode::ionian(),
        "minor" | "aeolian" => ScaleMode::aeolian(),
        "dorian" => ScaleMode::dorian(),
        "phrygian" => ScaleMode::phrygian(),
        "lydian" => ScaleMode::lydian(),
        "mixolydian" => ScaleMode::mixolydian(),
        "locrian" => ScaleMode::locrian(),
        _ => return None,
    };
    Some(Key::new(root, mode))
}

/// Build a key from a root name and a major/minor flag.
///
/// Exists so callers — the actions, the toolbar selector — can name a key
/// without depending on keyflow directly. `None` if the root isn't a
/// note.
#[must_use]
pub fn key_from_name(root: &str, major: bool) -> Option<Key> {
    let note = MusicalNote::from_string(&normalize_root(root))?;
    Some(if major {
        Key::major(note)
    } else {
        Key::minor(note)
    })
}

/// Find the KEY track, or make one.
fn ensure_key_track<D: Tracks>(daw: &D, project: ProjectContext) -> DawResult<TrackRef> {
    if let Some(track) = daw
        .all(project.clone())
        .into_iter()
        .find(|t| t.name.eq_ignore_ascii_case(KEY_TRACK))
    {
        return Ok(TrackRef::Guid(track.guid));
    }
    let guid = daw.add(project, KEY_TRACK, None)?;
    Ok(TrackRef::Guid(guid))
}

/// Place a key change at `seconds`.
///
/// Replaces any change already at that spot — setting the key twice at
/// one position should change it, not stack two labels on top of each
/// other.
///
/// # Errors
///
/// Returns an error if the item cannot be created on the KEY track.
pub fn set_key_at<D>(daw: &D, project: ProjectContext, seconds: f64, key: &Key) -> DawResult<()>
where
    D: Tracks + Items,
{
    let track = ensure_key_track(daw, project.clone())?;

    for item in daw.get_items(project.clone(), track.clone()) {
        if (item.position.as_seconds() - seconds).abs() < 0.001 {
            let _ = daw.delete_item(project.clone(), ItemRef::Guid(item.guid.clone()));
        }
    }

    let guid = daw
        .add_item(
            project.clone(),
            track,
            PositionInSeconds::from_seconds(seconds),
            Duration::from_seconds(MARKER_SECONDS),
        )
        .ok_or_else(|| {
            daw_proto::DawError::OperationFailed("could not create the key item".into())
        })?;

    let item = ItemRef::Guid(guid);
    daw.set_label(project.clone(), item.clone(), &format_key(key))?;
    // UFCS: `set_color` is on both Items and Tracks (E0034, issue #92).
    let _ = Items::set_color(daw, project, item, Some(KEY_COLOR));
    Ok(())
}

/// Every key change on the KEY track, earliest first.
pub fn key_changes<D>(daw: &D, project: &ProjectContext) -> Vec<KeyChange>
where
    D: Tracks + Items,
{
    let Some(track) = daw
        .all(project.clone())
        .into_iter()
        .find(|t| t.name.eq_ignore_ascii_case(KEY_TRACK))
    else {
        return Vec::new();
    };

    let mut changes: Vec<KeyChange> = daw
        .get_items(project.clone(), TrackRef::Guid(track.guid))
        .into_iter()
        .filter_map(|item| {
            let label = daw.label(project.clone(), ItemRef::Guid(item.guid.clone()))?;
            Some(KeyChange {
                seconds: item.position.as_seconds(),
                key: parse_key(&label)?,
            })
        })
        .collect();
    changes.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
    changes
}

/// The key in force at `seconds` — the latest change at or before it.
pub fn key_at<D>(daw: &D, project: &ProjectContext, seconds: f64) -> Option<Key>
where
    D: Tracks + Items,
{
    key_changes(daw, project)
        .into_iter()
        .rfind(|c| c.seconds <= seconds + 0.001)
        .map(|c| c.key)
}

/// Remove every key change from the KEY track, leaving the track itself.
///
/// Deletes only items whose label parses as a key — a note-to-self
/// parked on the track survives.
///
/// # Errors
///
/// Returns an error if an item cannot be deleted from the KEY track.
pub fn clear_key_changes<D>(daw: &D, project: &ProjectContext) -> DawResult<()>
where
    D: Tracks + Items,
{
    let Some(track) = daw
        .all(project.clone())
        .into_iter()
        .find(|t| t.name.eq_ignore_ascii_case(KEY_TRACK))
    else {
        return Ok(());
    };
    for item in daw.get_items(project.clone(), TrackRef::Guid(track.guid)) {
        let is_key = daw
            .label(project.clone(), ItemRef::Guid(item.guid.clone()))
            .and_then(|l| parse_key(&l))
            .is_some();
        if is_key {
            daw.delete_item(project.clone(), ItemRef::Guid(item.guid))?;
        }
    }
    Ok(())
}

/// REAPER's measure numbers into `<KEYSIG>`'s.
///
/// `TempoMap::time_to_musical` counts measures from 1, the way REAPER's
/// ruler does; the `<KEYSIG>` block counts from 0. Without this every
/// baked key signature lands one bar late — an error that looks right in
/// every unit test and wrong in every real project.
#[must_use]
pub fn keysig_measure(reaper_measure: i32) -> u32 {
    u32::try_from(reaper_measure.saturating_sub(1).max(0)).unwrap_or(0)
}

/// Convert a key into the `<KEYSIG>` triple: root pitch class, spelling,
/// and scale mask.
///
/// The spelling is taken from how the key is *written*, not computed —
/// `Db` asks for flats and `C#` for sharps, and they are the same pitch.
/// Losing that is losing the distinction the whole enharmonic split
/// exists for.
fn keysig_fields(key: &Key) -> (u8, i8, u32) {
    let accidental = if key.root.name.contains('b') {
        -1
    } else {
        i8::from(key.root.name.contains('#'))
    };
    // REAPER's mask describes the scale from its root. A minor key is the
    // major scale rotated, which REAPER spells by giving the relative
    // major's mask against the minor root — so both use the major mask.
    (
        key.root.semitone % 12,
        accidental,
        dawfile_reaper::keysig::SCALE_MASK_MAJOR,
    )
}

/// Write the KEY track's changes into the project file's `<KEYSIG>`
/// block, so REAPER's MIDI editor key snap agrees with them.
///
/// This exists because REAPER has no API for key signatures at all — not
/// in the C API, not among its 9,582 actions, and not in item or track
/// chunks (a Cockos thread, t=287164, has an extension author confirming
/// the last of those). The project file is the only representation, so
/// the only way in is through the file.
///
/// The cost is real and unavoidable: the project is saved, rewritten, and
/// reopened. Undo history does not survive that. It is deliberately a
/// separate, explicit action rather than something the per-key actions do
/// silently — you choose when to pay it.
///
/// Refuses to touch an unsaved project: without a path there is no file
/// to edit, and inventing one would put a user's work somewhere they
/// didn't ask for.
///
/// # Errors
///
/// Returns an error if the project is unsaved, if the project file cannot be read or written,
/// or if the project cannot be reopened after the file is modified.
pub fn bake_key_signatures<D>(daw: &D, project: &ProjectContext) -> DawResult<usize>
where
    D: Tracks + Items + Projects + TempoMap,
{
    let changes = key_changes(daw, project);
    if changes.is_empty() {
        return Ok(0);
    }

    let info = Projects::info(daw, project.clone())?;
    if info.path.trim().is_empty() {
        return Err(DawError::OperationFailed(
            "save the project first — baking rewrites its file".into(),
        ));
    }

    // Flush memory to disk, so what we read is what the user has.
    daw.save(project.clone());

    let sigs: Vec<dawfile_reaper::keysig::KeySig> = changes
        .iter()
        .map(|change| {
            let (measure, _, _) = daw.time_to_musical(project.clone(), change.seconds);
            let (root, accidental, scale_mask) = keysig_fields(&change.key);
            dawfile_reaper::keysig::KeySig {
                measure: keysig_measure(measure),
                root,
                accidental,
                scale_mask,
            }
        })
        .collect();

    let original = std::fs::read_to_string(&info.path)
        .map_err(|e| DawError::OperationFailed(format!("could not read {}: {e}", info.path)))?;
    let spliced = dawfile_reaper::keysig::splice(&original, &sigs);
    std::fs::write(&info.path, spliced)
        .map_err(|e| DawError::OperationFailed(format!("could not write {}: {e}", info.path)))?;

    // Reopen so REAPER picks the block up. Without this the edit sits on
    // disk and the next save overwrites it from memory.
    daw.open(&info.path)
        .ok_or_else(|| DawError::OperationFailed("could not reopen the project".into()))?;

    Ok(sigs.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_round_trip() {
        for text in ["C major", "Eb major", "F# minor", "Bb minor", "D dorian"] {
            let key = parse_key(text).unwrap_or_else(|| panic!("{text} should parse"));
            assert_eq!(format_key(&key), text, "{text} must survive the round trip");
        }
    }

    /// The label is what a person sees and edits, so it has to tolerate
    /// the ways a person writes it.
    #[test]
    fn parsing_is_forgiving_about_case_and_space() {
        assert_eq!(parse_key("  c   MAJOR "), parse_key("C major"));
        assert_eq!(parse_key("a Minor"), parse_key("A minor"));
    }

    /// Items on the KEY track that aren't key changes must be ignored,
    /// not guessed at — someone will put a note to themselves there.
    #[test]
    fn non_keys_are_ignored() {
        assert!(parse_key("fix this bit").is_none());
        assert!(parse_key("C").is_none(), "a bare root is not a key");
        assert!(parse_key("H major").is_none(), "H is not a note");
        assert!(parse_key("C sideways").is_none());
    }

    /// Ionian *is* major and Aeolian *is* minor — the same mode under
    /// two names. Both spellings must parse to the same key, and the
    /// common name is what gets written back, since that's what a
    /// musician reads.
    /// The 1-based / 0-based measure gap, pinned here as well as in the
    /// REAPER test that found it.
    #[test]
    fn reaper_measures_shift_down_by_one() {
        assert_eq!(keysig_measure(1), 0, "REAPER's bar 1 is KEYSIG measure 0");
        assert_eq!(keysig_measure(5), 4);
        assert_eq!(keysig_measure(0), 0, "never negative");
    }

    /// A key's spelling drives the KEYSIG accidental — that field is the
    /// only thing distinguishing Db major from C# major in the file.
    #[test]
    fn keysig_fields_carry_the_spelling() {
        let d_flat = parse_key("Db major").expect("Db major");
        let c_sharp = parse_key("C# major").expect("C# major");
        assert_eq!(keysig_fields(&d_flat), (1, -1, 0xAB5));
        assert_eq!(keysig_fields(&c_sharp), (1, 1, 0xAB5));

        let c = parse_key("C major").expect("C major");
        assert_eq!(keysig_fields(&c), (0, 0, 0xAB5), "naturals spell as 0");
    }

    #[test]
    fn ionian_and_major_are_the_same_key() {
        assert_eq!(parse_key("C ionian"), parse_key("C major"));
        assert_eq!(parse_key("A aeolian"), parse_key("A minor"));

        let from_ionian = parse_key("C ionian").expect("C ionian");
        assert_eq!(
            format_key(&from_ionian),
            "C major",
            "written as the common name"
        );
    }

    #[test]
    fn relative_keys_are_distinct_despite_sharing_pitches() {
        let c = parse_key("C major").expect("C major");
        let a = parse_key("A minor").expect("A minor");
        assert_ne!(c, a);
    }
}

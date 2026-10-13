//! What colour a track *should* be.
//!
//! Names go through `monarchy_sort` — the same classifier the track
//! organiser uses to decide a track's group — and the resulting group
//! path is looked up in `music_catalog`'s palette. Colour and grouping
//! therefore agree by construction: a track that sorts into
//! `Guitars/Electric` gets the Electric Guitar colour because that is
//! what it *is*, not because a separate substring rule happened to match
//! the same name.
//!
//! That's the whole reason this replaced the previous hand-rolled rule
//! table (`has_drums()` / `has_bass()` / … over a normalised name): the
//! two classifiers disagreed, and only one of them was the one the rest
//! of the app used.
//!
//! Ported from `dynamic_template::auto_color`, which had this half and no
//! runtime.

use std::collections::HashMap;
use std::hash::BuildHasher;

use color_palette::Color;
use monarchy::Metadata;

/// Map every track name to the colour of the group it classifies into.
///
/// Names that don't classify, or whose group has no colour, are absent
/// from the map — the caller decides what to do with them (the runtime
/// falls back to inheriting the nearest coloured parent).
#[must_use]
pub fn classify_and_color(track_names: Vec<String>) -> HashMap<String, Color> {
    let config = dynamic_template::default_config();
    let Ok(structure) = monarchy::monarchy_sort(track_names, &config) else {
        return HashMap::new();
    };

    let mut color_map = HashMap::new();
    collect_colors_from_structure(&structure, &[], &mut color_map);
    color_map
}

/// The runtime's entry point: track name → packed RGB.
///
/// Keyed by name rather than guid because that's what the classifier
/// works in. Two tracks sharing a name share a colour, which is the
/// intent — they're the same thing twice.
#[must_use]
pub fn colors_by_track_name(tracks: &[daw::service::Track]) -> HashMap<String, u32> {
    let names: Vec<String> = tracks.iter().map(|track| track.name.clone()).collect();
    let mut out: HashMap<String, u32> = classify_and_color(names)
        .into_iter()
        .map(|(name, color)| (name, color.to_hex()))
        .collect();

    // `monarchy_sort` classifies *items* into groups, so it has nothing to
    // say about a folder track literally named after a group ("Guitars",
    // "Drums"). Those are exactly the tracks a user most expects to be
    // coloured, so fall back to a direct palette lookup on the name.
    for track in tracks {
        if out.contains_key(track.name.as_str()) {
            continue;
        }
        if let Some(color) = music_catalog::lookup::color_for_name(&track.name) {
            out.insert(track.name.clone(), color.to_hex());
        }
    }
    out
}

/// Walk the sorted structure, assigning each item the colour of the
/// deepest group it landed in. Children are visited after parents so a
/// more specific group's colour wins.
fn collect_colors_from_structure<M: Metadata>(
    structure: &monarchy::Structure<M>,
    parent_path: &[&str],
    color_map: &mut HashMap<String, Color>,
) {
    let mut current_path: Vec<&str> = parent_path.to_vec();
    if !structure.name.is_empty() && structure.name != "root" {
        current_path.push(&structure.name);
    }

    if let Some(color) = dynamic_template::colors::color_for_path(&current_path) {
        for item in &structure.items {
            color_map.insert(item.original.clone(), color);
        }
    }

    for child in &structure.children {
        collect_colors_from_structure(child, &current_path, color_map);
    }
}

// ── Backend-agnostic application ────────────────────────────────────────
//
// The runtime in the parent module drives REAPER directly (it needs
// ExtState and the main-thread bridge). These are the plain
// `impl Tracks` versions: no persistence, no reactivity, testable against
// `daw_standalone`. Useful on their own, and the seam any future
// section-aware or setlist-aware colouring should apply through.

use daw::service::{ProjectContext, Track, TrackRef, Tracks};

/// Classify every track in `project` and paint it. Returns how many
/// tracks were coloured.
pub fn apply_colors(service: &impl Tracks, project: &ProjectContext) -> u32 {
    let tracks = service.all(project.clone());
    if tracks.is_empty() {
        return 0;
    }
    let names: Vec<String> = tracks.iter().map(|t| t.name.clone()).collect();
    let color_map = classify_and_color(names);
    apply_color_map(service, project, &tracks, &color_map)
}

/// Paint a pre-computed map. Split out from [`apply_colors`] so a caller
/// with its own opinion about colour — a section-aware pass, say — can
/// reuse the application half.
pub fn apply_color_map<S: BuildHasher>(
    service: &impl Tracks,
    project: &ProjectContext,
    tracks: &[Track],
    color_map: &HashMap<String, Color, S>,
) -> u32 {
    let mut colored: u32 = 0;
    for track in tracks {
        if let Some(color) = color_map.get(&track.name)
            && service
                .set_color(
                    project.clone(),
                    TrackRef::Guid(track.guid.clone()),
                    color.to_hex(),
                )
                .is_ok()
        {
            colored = colored.saturating_add(1);
        }
    }
    colored
}

/// Reset every track in `project` to the DAW's default colour.
pub fn clear_colors(service: &impl Tracks, project: &ProjectContext) -> u32 {
    let mut cleared: u32 = 0;
    for track in &service.all(project.clone()) {
        if service
            .set_color(project.clone(), TrackRef::Guid(track.guid.clone()), 0)
            .is_ok()
        {
            cleared = cleared.saturating_add(1);
        }
    }
    cleared
}

#[cfg(test)]
mod tests {
    use super::*;
    use dynamic_template::colors;

    /// Marc Martel "Don't Stop Me Now" — a real multitrack name list.
    /// Ported from dynamic-template's `05_auto_color.rs`, which was never
    /// wired into that crate's `[[test]]` list and so had never actually
    /// run: it still called an async `apply_colors` and a `TrackService`
    /// trait that no longer exist.
    fn marc_martel_track_names() -> Vec<String> {
        [
            "Kick In",
            "Kick Out",
            "Kick Sample",
            "Snare Top",
            "Snare Bottom",
            "Snare Sample",
            "Snare Sample Two",
            "Tom1",
            "Tom2",
            "Tom3",
            "Overhead L",
            "Overhead R",
            "Room L",
            "Room R",
            "Hat",
            "Bass DI",
            "Bass Amp",
            "Gtr L",
            "Gtr R",
            "Gtr Solo",
            "Piano",
            "Organ",
            "Vocal",
            "BGV1",
            "BGV2",
            "BGV3",
        ]
        .into_iter()
        .map(String::from)
        .collect()
    }

    #[test]
    fn classifies_a_real_multitrack_list() {
        let color_map = classify_and_color(marc_martel_track_names());
        assert!(
            color_map.len() >= 20,
            "expected at least 20 classified tracks, got {}",
            color_map.len()
        );
        for name in ["Kick In", "Snare Top", "Bass DI", "Piano", "Vocal", "BGV1"] {
            assert!(color_map.contains_key(name), "'{name}' should classify");
        }
    }

    /// The point of using monarchy rather than a private rule table: a
    /// track gets its *group's* colour, so colour and grouping can't drift
    /// apart.
    #[test]
    fn group_colors_come_from_the_shared_palette() {
        let color_map = classify_and_color(marc_martel_track_names());
        assert_eq!(
            color_map.get("Bass DI").map(|c| c.to_hex()),
            Some(colors::groups::BASS.to_hex())
        );
        assert_eq!(
            color_map.get("Piano").map(|c| c.to_hex()),
            Some(colors::keys::PIANO.to_hex())
        );
    }

    #[test]
    fn empty_list_classifies_to_nothing() {
        assert!(classify_and_color(vec![]).is_empty());
    }
}

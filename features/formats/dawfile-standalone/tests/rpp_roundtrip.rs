//! The acceptance test for #156: the lossless claim, *proven*.
//!
//! The corpus is `dawfile-reaper`'s fixture set, which is the one body of
//! real REAPER project text the tree ships. Three claims are asserted over
//! all of it:
//!
//! 1. **Byte-faithfulness for untouched projects.** `rpp → .daw → rpp`
//!    returns the original bytes exactly.
//! 2. **The patch path is faithful too.** Exporting an *unedited* document
//!    through the patching path — not the verbatim shortcut — rewrites
//!    nothing and produces text that re-parses to the same tree. This is the
//!    stronger claim: it means fidelity does not depend on the shortcut.
//! 3. **Semantic correctness for an edited project.** An edit made through
//!    the typed API shows up in the exported `.rpp`, and everything the edit
//!    did not touch is still there.
//!
//! Plus the standing structural checks: every entity has a stable id, no
//! duplicates, no dangling parent, and the `.daw` text round-trips.
//!
//! ## Extending the corpus
//!
//! Point `DAW_RPP_CORPUS` at a directory of `.RPP` files to run every claim
//! over real sessions as well. It is opt-in because the test must be
//! deterministic on a fresh checkout, and the real material (#159) is
//! ~13.8 GB that no CI machine has.

use dawfile_standalone::{
    DawProject, DocumentEdit, DocumentQuery, EntityId, ImportReport, SourceRef,
};
use std::path::{Path, PathBuf};

/// Every `.RPP` in the fixture corpus, plus anything `DAW_RPP_CORPUS` adds.
fn corpus() -> Vec<PathBuf> {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../dawfile-reaper/tests/fixtures")
        .canonicalize()
        .expect("dawfile-reaper fixtures are part of the tree");

    let mut found = Vec::new();
    collect_rpp(&fixtures, &mut found);
    if let Ok(extra) = std::env::var("DAW_RPP_CORPUS") {
        collect_rpp(Path::new(&extra), &mut found);
    }
    found.sort();
    assert!(
        !found.is_empty(),
        "the corpus is empty — the fixtures moved"
    );
    found
}

fn collect_rpp(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rpp(&path, out);
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("rpp"))
        {
            out.push(path);
        }
    }
}

fn label(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

// ──────────────────────────────────────────────────────────────
// 1. Byte-faithfulness for untouched projects
// ──────────────────────────────────────────────────────────────

#[test]
fn an_untouched_project_exports_back_byte_for_byte() {
    for path in corpus() {
        let original = std::fs::read_to_string(&path).expect("read fixture");
        let (project, _) = DawProject::import_rpp_file(&path).expect("import");

        assert!(
            !project.is_modified(),
            "{}: import must not mark the project modified",
            label(&path)
        );

        let exported = project.to_rpp().expect("export");
        assert_eq!(
            exported,
            original,
            "{}: untouched round trip is not byte-identical",
            label(&path)
        );
    }
}

// ──────────────────────────────────────────────────────────────
// 2. The patch path is faithful without the verbatim shortcut
// ──────────────────────────────────────────────────────────────

#[test]
fn patching_an_unedited_document_rewrites_nothing() {
    for path in corpus() {
        let (project, _) = DawProject::import_rpp_file(&path).expect("import");
        let (_, report) = project.to_rpp_patched().expect("patched export");

        assert!(
            report.is_empty(),
            "{}: patching an unedited document rewrote {} token(s):\n  {}",
            label(&path),
            report.changes.len(),
            report.changes.join("\n  ")
        );
    }
}

#[test]
fn the_patch_path_preserves_every_line_of_the_original() {
    // Byte equality is not available through the patch path — the chunk tree
    // normalises quoting and indentation on the way out, exactly as
    // `dawfile_reaper`'s own round-trip test documents. What must hold is
    // that no *content* moved: same structure, same tokens, same order.
    for path in corpus() {
        let original = std::fs::read_to_string(&path).expect("read fixture");
        let (project, _) = DawProject::import_rpp_file(&path).expect("import");
        let patched = project.to_rpp_patched().expect("patched export").0;

        assert_eq!(
            token_signature(&original),
            token_signature(&patched),
            "{}: the patch path changed the project's content",
            label(&path)
        );
    }
}

/// Every line's token sequence, in order, with quoting normalised away.
///
/// This is the comparison that actually means "nothing was lost": it sees
/// through the tree's re-quoting but not through a dropped line, a reordered
/// chunk or a changed value.
fn token_signature(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            dawfile_reaper::rpp_tree::tokenize(line.trim_start_matches('<'))
                .into_iter()
                .map(|token| token.token)
                .collect()
        })
        .collect()
}

// ──────────────────────────────────────────────────────────────
// 3. Semantic correctness for an edited project
// ──────────────────────────────────────────────────────────────

#[test]
fn an_edited_track_survives_the_round_trip_semantically() {
    for path in corpus() {
        let (mut project, _) = DawProject::import_rpp_file(&path).expect("import");
        let Some(track) = project
            .document()
            .tracks
            .first()
            .map(|node| node.id.clone())
        else {
            continue;
        };
        let original_name = project
            .document()
            .track(&track)
            .expect("track")
            .track
            .name
            .clone();

        project.edit(|document| {
            let node = document.track_mut(&track).expect("track");
            node.track.volume = 0.375;
            node.track.muted = true;
            node.track.name = format!("{original_name} [edited]");
        });

        let exported = project.to_rpp().expect("export");
        let (reimported, _) = DawProject::import_rpp(&exported, "reimported").expect("re-import");

        let node = reimported.document().track(&track).expect("track survived");
        assert_eq!(node.track.volume, 0.375, "{}: volume", label(&path));
        assert!(node.track.muted, "{}: mute", label(&path));
        assert_eq!(
            node.track.name,
            format!("{original_name} [edited]"),
            "{}: name",
            label(&path)
        );

        // Everything else is still there: same tracks, same ids, same order.
        let before: Vec<&EntityId> = project.document().tracks.iter().map(|n| &n.id).collect();
        let after: Vec<&EntityId> = reimported.document().tracks.iter().map(|n| &n.id).collect();
        assert_eq!(before, after, "{}: track set changed", label(&path));
    }
}

#[test]
fn an_edit_rewrites_only_what_it_touched() {
    // The point of patching rather than regenerating: a one-value edit is a
    // one-token diff, so `.rpp` files stay diffable and REAPER-specific
    // constructs never get reformatted out from under the user.
    let path = corpus()
        .into_iter()
        .find(|path| label(path) == "song_a.RPP")
        .expect("song_a fixture");
    let original = std::fs::read_to_string(&path).expect("read");

    let (mut project, _) = DawProject::import_rpp_file(&path).expect("import");
    let track = project.document().tracks[0].id.clone();
    project.edit(|document| document.track_mut(&track).expect("track").track.volume = 0.5);

    let (exported, report) = project.to_rpp_patched().expect("export");
    assert_eq!(
        report.changes.len(),
        1,
        "expected exactly one rewritten token, got:\n  {}",
        report.changes.join("\n  ")
    );

    let differing = token_signature(&original)
        .into_iter()
        .zip(token_signature(&exported))
        .filter(|(before, after)| before != after)
        .count();
    assert_eq!(differing, 1, "an edit should touch exactly one line");
}

#[test]
fn structural_additions_and_removals_reach_the_exported_rpp() {
    let path = corpus()
        .into_iter()
        .find(|path| label(path) == "song_a.RPP")
        .expect("song_a fixture");
    let (mut project, _) = DawProject::import_rpp_file(&path).expect("import");

    let doomed = project.document().tracks[0].id.clone();
    let (added_track, added_item) = project.edit(|document| {
        document.remove_track(&doomed).expect("remove");
        let track = document.add_track("Bounced Stem");
        let item = document.add_item(&track, 4.0, 12.0).expect("item");
        document
            .add_take(
                &item,
                SourceRef::File {
                    path: "Media/stem.wav".into(),
                    kind: "WAVE".into(),
                },
            )
            .expect("take");
        (track, item)
    });

    let exported = project.to_rpp().expect("export");
    let (reimported, _) = DawProject::import_rpp(&exported, "reimported").expect("re-import");
    let document = reimported.document();

    assert!(
        document.track(&doomed).is_none(),
        "a removed track came back from the dead"
    );
    let track = document
        .track(&added_track)
        .expect("the added track reached the .rpp");
    assert_eq!(track.track.name, "Bounced Stem");
    let item = document
        .item(&added_item)
        .expect("the added item reached the .rpp");
    assert_eq!(item.item.position.as_seconds(), 4.0);
    assert_eq!(item.item.length.as_seconds(), 12.0);
    assert_eq!(item.takes.len(), 1);
    assert!(matches!(
        item.takes[0].source,
        SourceRef::File { .. } | SourceRef::Empty
    ));
    assert!(document.check_invariants().is_empty());
}

#[test]
fn edited_envelope_points_reach_the_exported_rpp() {
    use daw_proto::automation::{EnvelopeShape, EnvelopeType};
    use daw_proto::primitives::PositionInSeconds;

    let path = corpus()
        .into_iter()
        .find(|path| label(path) == "song_a.RPP")
        .expect("song_a fixture");
    let (mut project, _) = DawProject::import_rpp_file(&path).expect("import");

    let track = project.document().tracks[0].id.clone();
    let envelope = project.edit(|document| {
        let envelope = document
            .add_track_envelope(&track, EnvelopeType::Volume, "VOLENV2")
            .expect("envelope");
        document
            .set_envelope_points(
                &envelope,
                vec![
                    daw_proto::automation::EnvelopePoint {
                        index: 0,
                        time: PositionInSeconds::from_seconds(8.0),
                        value: 0.25,
                        shape: EnvelopeShape::Linear,
                        tension: 0.0,
                        selected: false,
                    },
                    daw_proto::automation::EnvelopePoint {
                        index: 1,
                        time: PositionInSeconds::from_seconds(2.0),
                        value: 1.0,
                        shape: EnvelopeShape::Square,
                        tension: 0.0,
                        selected: false,
                    },
                ],
            )
            .expect("points");
        envelope
    });

    let exported = project.to_rpp().expect("export");
    let (reimported, _) = DawProject::import_rpp(&exported, "reimported").expect("re-import");

    let node = reimported
        .document()
        .envelope(&envelope)
        .expect("the envelope reached the .rpp and kept its EGUID");
    assert_eq!(node.points.len(), 2);
    // Sorted on the way in, so the exported `PT` lines are in time order.
    assert_eq!(node.points[0].time.as_seconds(), 2.0);
    assert_eq!(node.points[0].value, 1.0);
    assert_eq!(node.points[1].value, 0.25);
}

// ──────────────────────────────────────────────────────────────
// Structural invariants over the whole corpus
// ──────────────────────────────────────────────────────────────

#[test]
fn every_entity_in_the_corpus_has_a_stable_unique_id() {
    for path in corpus() {
        let (project, _) = DawProject::import_rpp_file(&path).expect("import");
        let problems = project.document().check_invariants();
        assert!(
            problems.is_empty(),
            "{}: {}",
            label(&path),
            problems.join("; ")
        );
    }
}

#[test]
fn importing_twice_yields_the_same_ids() {
    // Ids are adopted from the source's own GUIDs, or derived
    // deterministically. Re-importing must therefore not look like a
    // delete-and-recreate — which is what would happen with minted ids, and
    // what would make offline merge (#173) useless.
    for path in corpus() {
        let (first, _) = DawProject::import_rpp_file(&path).expect("import");
        let (second, _) = DawProject::import_rpp_file(&path).expect("re-import");

        let ids = |project: &DawProject| -> Vec<String> {
            project
                .document()
                .tracks
                .iter()
                .flat_map(|track| {
                    std::iter::once(track.id.to_string())
                        .chain(track.envelopes.iter().map(|e| e.id.to_string()))
                        .chain(track.items.iter().flat_map(|item| {
                            std::iter::once(item.id.to_string())
                                .chain(item.takes.iter().map(|take| take.id.to_string()))
                        }))
                })
                .collect()
        };
        assert_eq!(ids(&first), ids(&second), "{}", label(&path));
    }
}

#[test]
fn a_corpus_project_survives_a_daw_file_round_trip() {
    for path in corpus() {
        let (mut project, _) = DawProject::import_rpp_file(&path).expect("import");
        let dir = std::env::temp_dir().join(format!("daw-corpus-{}", uuid::Uuid::new_v4()));
        project.save(&dir).expect("save");

        let reloaded = DawProject::load(&dir).expect("load");
        let before = project.document();
        let after = reloaded.document();

        assert_eq!(after.tracks.len(), before.tracks.len(), "{}", label(&path));
        assert_eq!(
            after.markers.len(),
            before.markers.len(),
            "{}",
            label(&path)
        );
        assert_eq!(
            after.tempo_map.len(),
            before.tempo_map.len(),
            "{}",
            label(&path)
        );
        for (before_track, after_track) in before.tracks.iter().zip(&after.tracks) {
            assert_eq!(before_track.id, after_track.id, "{}", label(&path));
            assert_eq!(
                before_track.track.name,
                after_track.track.name,
                "{}",
                label(&path)
            );
            assert_eq!(
                before_track.items.len(),
                after_track.items.len(),
                "{}",
                label(&path)
            );
        }

        // And a reloaded project still exports byte-for-byte, which is the
        // whole point of keeping the verbatim source in `objects/`.
        let original = std::fs::read_to_string(&path).expect("read");
        assert_eq!(
            reloaded.to_rpp().expect("export"),
            original,
            "{}",
            label(&path)
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}

// ──────────────────────────────────────────────────────────────
// What the corpus surfaced that the schema did not anticipate
// ──────────────────────────────────────────────────────────────

#[test]
fn the_corpus_reports_what_it_did_not_model() {
    // Not an assertion that the list is empty — it never will be, because
    // FX chains and MIDI sources are deliberately opaque and REAPER carries
    // dozens of UI-state keys the editor has no use for. The assertion is
    // that the list is *visible*: a new construct shows up here rather than
    // vanishing. Print it so a failing widen-the-schema decision has data.
    let mut combined = ImportReport::default();
    for path in corpus() {
        let (_, report) = DawProject::import_rpp_file(&path).expect("import");
        for (key, count) in &report.unmodelled {
            *combined.unmodelled.entry(key.clone()).or_insert(0) += count;
        }
        combined.opaque_objects += report.opaque_objects;
    }

    println!("unmodelled constructs across the corpus:");
    for (key, count) in combined.ranked() {
        println!("  {key:<32} x{count}");
    }
    println!("chunks carried opaquely: {}", combined.opaque_objects);

    // The things that must *not* be unmodelled, because the editor edits
    // them. A regression that stops decoding one of these would otherwise
    // pass every other test in this file.
    for key in [
        "track/NAME",
        "track/VOLPAN",
        "track/MUTESOLO",
        "item/POSITION",
        "item/LENGTH",
        "item/IGUID",
        "project/MARKER",
    ] {
        assert!(
            !combined.unmodelled.contains_key(key),
            "{key} stopped being modelled"
        );
    }
}

#[test]
fn the_corpus_actually_decodes_the_modelled_set() {
    // A round-trip test passes trivially if the importer models nothing at
    // all, so this pins the floor: the corpus must yield real tracks, real
    // items, a real tempo map and real markers.
    let mut tracks = 0;
    let mut items = 0;
    let mut takes = 0;
    let mut markers = 0;
    let mut tempo_points = 0;
    let mut envelopes = 0;

    for path in corpus() {
        let (project, _) = DawProject::import_rpp_file(&path).expect("import");
        let document = project.document();
        tracks += document.tracks.len();
        markers += document.markers.len();
        tempo_points += document.tempo_map.len();
        for track in &document.tracks {
            envelopes += track.envelopes.len();
            items += track.items.len();
            for item in &track.items {
                takes += item.takes.len();
                envelopes += item.envelopes.len();
            }
        }
    }

    assert!(tracks > 0, "no tracks decoded");
    assert!(items > 0, "no items decoded");
    assert!(takes > 0, "no takes decoded");
    assert!(markers > 0, "no markers decoded");
    assert!(tempo_points > 0, "no tempo points decoded");
    println!(
        "corpus: {tracks} tracks, {items} items, {takes} takes, \
         {envelopes} envelopes, {markers} markers, {tempo_points} tempo points"
    );
}

// ──────────────────────────────────────────────────────────────
// Decoding details the corpus tests would not catch on their own
// ──────────────────────────────────────────────────────────────

const MARKERS: &str = r#"<REAPER_PROJECT 0.1 "7.0/test" 1700000000
  MARKER 1 0 PREROLL 0 0 1 B {00000000-0000-0000-0000-00000000A001} 0
  MARKER 5 4 Verse 1 16576 1 B {00000000-0000-0000-0000-00000000A005} 0
  MARKER 5 12 "" 1
>
"#;

#[test]
fn a_region_is_a_pair_of_marker_lines_not_a_flag_bit() {
    // Markers and regions are the same record in REAPER; a region is
    // recognised by a second `MARKER` line sharing the first's id. Reading
    // field 4 as an "is region" bit passes on projects where every region
    // happens to be flagged and mis-classifies everything else, so this pins
    // the actual rule.
    let (project, _) = DawProject::import_rpp(MARKERS, "markers").expect("import");
    let markers = &project.document().markers;

    assert_eq!(markers.len(), 2, "the region's two lines are one entity");

    let preroll = &markers[0];
    assert_eq!(preroll.marker.name, "PREROLL");
    assert_eq!(
        preroll.region_end_seconds, None,
        "a point marker has no end"
    );

    let verse = &markers[1];
    assert_eq!(
        verse.marker.name, "Verse",
        "the closing line must not blank the name"
    );
    assert_eq!(verse.region_end_seconds, Some(12.0));
    // Field 5 is the colour; field 4 is flags. Reading the wrong one gives
    // every marker in the corpus a colour of 1.
    assert_eq!(verse.marker.color, Some(16576));
    assert_eq!(
        preroll.marker.color, None,
        "colour 0 means 'default', not black"
    );
    // Both halves share one stable id, taken from the source's own GUID.
    assert_eq!(verse.id.as_str(), "{00000000-0000-0000-0000-00000000A005}");
}

const MIXED_ITEM: &str = r#"<REAPER_PROJECT 0.1 "7.0/test" 1700000000
  <TRACK {AAAAAAAA-0001-0000-0000-000000000000}
    NAME Vox
    VOLPAN 0.5 -0.25 -1 -1 1
    TRACKID {AAAAAAAA-0001-0000-0000-000000000000}
    <ITEM
      POSITION 8
      LENGTH 4
      IGUID {AAAAAAAA-I001-0000-0000-000000000000}
      NAME Comp
      VOLPAN 0.75 0.1 0.25 -1
      SOFFS 1.5
      PLAYRATE 1 1 0 -1 0 0.0025
      GUID {AAAAAAAA-T001-0000-0000-000000000000}
    >
  >
>
"#;

#[test]
fn item_trim_and_take_volume_are_read_from_the_right_fields() {
    // `VOLPAN <item trim> <take pan> <take volume> <take pan law>` sits in
    // the take's run but its first field belongs to the item. Getting this
    // wrong is invisible on a default project — every field is 1.0 — and
    // wrong on every project anyone has actually mixed, which is why the
    // fixture uses four distinct values.
    let (project, _) = DawProject::import_rpp(MIXED_ITEM, "mixed").expect("import");
    let document = project.document();

    assert_eq!(document.tracks[0].track.volume, 0.5, "track volume");
    assert_eq!(document.tracks[0].track.pan, -0.25, "track pan");

    let item = &document.tracks[0].items[0];
    assert_eq!(item.item.volume, 0.75, "item trim is VOLPAN field 1");
    assert_eq!(
        item.takes[0].take.volume, 0.25,
        "take volume is VOLPAN field 3"
    );
    assert_eq!(item.takes[0].take.start_offset.as_seconds(), 1.5);
}

#[test]
fn editing_item_trim_and_take_volume_writes_the_right_fields() {
    let (mut project, _) = DawProject::import_rpp(MIXED_ITEM, "mixed").expect("import");
    let item = project.document().tracks[0].items[0].id.clone();
    let take = project.document().tracks[0].items[0].takes[0].id.clone();

    project.edit(|document| {
        document.item_mut(&item).expect("item").item.volume = 0.9;
        document.take_mut(&take).expect("take").take.volume = 0.1;
    });

    let exported = project.to_rpp().expect("export");
    let (reimported, _) = DawProject::import_rpp(&exported, "reimported").expect("re-import");

    assert_eq!(
        reimported.document().item(&item).expect("item").item.volume,
        0.9
    );
    assert_eq!(
        reimported.document().take(&take).expect("take").take.volume,
        0.1
    );
    // And the neighbouring fields on the same line are untouched.
    assert!(
        exported.contains("VOLPAN 0.9 0.1 0.1 -1"),
        "the take pan and pan law should be left alone:\n{exported}"
    );
}

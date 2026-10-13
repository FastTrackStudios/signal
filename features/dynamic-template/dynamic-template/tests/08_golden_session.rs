//! The golden session is what the builder builds, and it carries what
//! the checklist says it carries.
//!
//! `docs/spec/session/maximal-template.md` is the reference session's
//! spec; `dynamic_template::golden_session` builds it from the
//! config-derived template and a fixture song shape; the built project
//! files are committed under `features/dynamic-template/fixtures/golden/`.
//! These tests hold the three together:
//!
//! - the builder's output equals the committed files, so a change to the
//!   builder or the template is a diff in a PR (`just daw-template`
//!   regenerates);
//! - every template-created track carries its taxonomy kind in ext-state
//!   and it reads back;
//! - every shape the checklist ticks holds in the built tree, and the
//!   checklist in the doc is exactly what the checks regenerate — never
//!   hand-ticked.

use std::path::PathBuf;

use dynamic_template::golden_session::checklist::{self, Golden};
use dynamic_template::golden_session::{build, fixtures, fixtures_dir, read_kinds, rpp_file, Kind};
use dynamic_template::golden_template;

type Result<T> = core::result::Result<T, Box<dyn std::error::Error>>;

fn committed(file: &str) -> Result<String> {
    let path = fixtures_dir().join(file);
    Ok(std::fs::read_to_string(&path)
        .map_err(|e| format!("{}: {e} — run `just daw-template`", path.display()))?)
}

#[test]
fn the_builder_reproduces_every_committed_project_file() -> Result<()> {
    for shape in fixtures() {
        let file = rpp_file(&shape);
        let built = build(&shape).rpp;
        let on_disk = committed(&file)?;
        if built != on_disk {
            // The first differing line, so the failure says what moved
            // rather than dumping two hundred kilobytes.
            let (n, a, b) = built
                .lines()
                .zip(on_disk.lines())
                .enumerate()
                .find(|(_, (a, b))| a != b)
                .map_or((0, "", ""), |(n, (a, b))| (n.saturating_add(1), a, b));
            panic!(
                "{file} is stale: run `just daw-template` and commit the result.\n\
                 first difference at line {n}:\n  built:     {a}\n  committed: {b}"
            );
        }
    }
    Ok(())
}

#[test]
fn every_template_created_track_carries_a_kind_that_reads_back() -> Result<()> {
    for shape in fixtures() {
        let text = committed(&rpp_file(&shape))?;
        let tracks = read_kinds(&text);
        assert_eq!(tracks.len(), build(&shape).tracks.len(), "{}", shape.name);
        let untagged: Vec<&str> = tracks
            .iter()
            .filter(|t| t.kind.is_none())
            .map(|t| t.name.as_str())
            .collect();
        assert!(
            untagged.is_empty(),
            "{}: tracks with no kind: {untagged:?}",
            shape.name
        );
        // Every kind is used for what it says: a bus is a bus, a Sum is
        // a Sum, and the template groups say which group they stand for.
        let kinds_of = |name: &str| -> Vec<Option<Kind>> {
            tracks
                .iter()
                .filter(|t| t.name == name)
                .map(|t| t.kind)
                .collect()
        };
        if shape.name == "template" {
            // The root of the bus tree is its own kind, so a scene can
            // say "the mix bus tree, out of the way" in one rule; the
            // buses under it are ordinary buses.
            assert_eq!(kinds_of("MIX BUS"), vec![Some(Kind::MixBus)]);
            assert_eq!(kinds_of("DRUM BUS"), vec![Some(Kind::Bus)]);
            assert!(kinds_of("Sum").iter().all(|k| *k == Some(Kind::Sum)));
            assert_eq!(kinds_of("Process"), vec![Some(Kind::Process)]);
            assert_eq!(kinds_of("Compress"), vec![Some(Kind::Compress)]);
            let kick = tracks.iter().find(|t| t.name == "Kick");
            assert_eq!(kick.map(|t| t.kind), Some(Some(Kind::Piece)));
            assert_eq!(
                kick.and_then(|t| t.group.as_deref()),
                Some("Drums/Drum Kit/Kick")
            );
        }
    }
    Ok(())
}

/// The negative control for the reader: a REAPER project no template
/// touched has tracks, and none of them has a kind.
#[test]
fn a_project_the_template_did_not_create_has_no_kinds() {
    let text = "<REAPER_PROJECT 0.1 \"7.0\" 0\n  <TRACK {A}\n    NAME \"Kick\"\n  >\n  <TRACK {B}\n    NAME \"MIX BUS\"\n  >\n>\n";
    let tracks = read_kinds(text);
    assert_eq!(tracks.len(), 2);
    assert!(tracks.iter().all(|t| t.kind.is_none()));
}

#[test]
fn the_fixtures_stand_for_template_groups_that_exist() {
    let template = golden_template();
    for shape in fixtures() {
        let drift = shape.resolve(&template);
        assert!(drift.is_empty(), "{}: {drift:?}", shape.name);
    }
}

/// Every shape the checklist ticks holds in the built tree, one line
/// per failure so a regression names the shape it broke.
#[test]
fn every_ticked_shape_holds_in_the_built_tree() {
    let golden = Golden::load(&fixtures_dir());
    let failed: Vec<&str> = checklist::ticks(&golden)
        .into_iter()
        .filter(|(_, ok)| !ok)
        .map(|(text, _)| text)
        .collect();
    assert!(
        failed.is_empty(),
        "shapes that no longer hold:\n{}",
        failed.join("\n---\n")
    );
    // Not vacuous: the checks cover the drums, the guitars, the buses.
    assert!(checklist::ticks(&golden).len() >= 30);
}

#[test]
fn the_checklist_in_the_doc_is_what_the_checks_regenerate() -> Result<()> {
    let path: PathBuf = checklist::doc_path();
    let doc = std::fs::read_to_string(&path)?;
    let regenerated = checklist::regenerate(&doc, &Golden::load(&fixtures_dir()))?;
    if regenerated != doc {
        let (n, a, b) = regenerated
            .lines()
            .zip(doc.lines())
            .enumerate()
            .find(|(_, (a, b))| a != b)
            .map_or((0, "", ""), |(n, (a, b))| (n.saturating_add(1), a, b));
        panic!(
            "{} is stale: the checklist is regenerated from the golden-rule checks, \
             never hand-ticked — run `just daw-template` and commit the result.\n\
             first difference at line {n}:\n  checks: {a}\n  doc:    {b}",
            path.display()
        );
    }
    Ok(())
}

/// A tick the checks did not earn is stale too: the doc with one more
/// box ticked fails the same comparison.
#[test]
fn a_hand_tick_is_stale() -> Result<()> {
    let doc = std::fs::read_to_string(checklist::doc_path())?;
    let forged = doc.replacen("- [ ] **Keyflow/**", "- [x] **Keyflow/**", 1);
    assert_ne!(
        forged, doc,
        "the Keyflow item is expected unticked in this shape"
    );
    assert_ne!(
        checklist::regenerate(&forged, &Golden::load(&fixtures_dir()))?,
        forged
    );
    Ok(())
}

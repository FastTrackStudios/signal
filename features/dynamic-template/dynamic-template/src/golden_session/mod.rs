//! The golden session: the template applying itself to a fixture album.
//!
//! `docs/spec/session/maximal-template.md` describes the reference
//! session every scene is rendered against. This module builds it — in
//! Rust, through `dawfile-reaper`'s builders — from the config-derived
//! template ([`golden_template`](crate::golden::golden_template)) and a
//! fixture song shape ([`shape`]), so the template and its golden cannot
//! drift apart without a test noticing.
//!
//! The fixtures live under `features/dynamic-template/fixtures/golden/`:
//! the two project files are committed, the scene renders beside them
//! are committed, and the media the projects reference is generated.
//! `just daw-template` regenerates all of it; a test fails when the
//! committed text is stale.

pub mod checklist;
pub mod guid;
pub mod kind;
pub mod media;
pub mod perform;
pub mod rpp;
pub mod shape;
pub mod song;

use std::path::{Path, PathBuf};

pub use guid::guid;
pub use kind::{read_kinds, Kind, TrackExt, GROUP_KEY, KIND_KEY};
pub use rpp::{build, Built, Flat};
pub use shape::{maximal, vocal_fx, Shape};

/// The fixtures directory, resolved from this crate's manifest.
#[must_use]
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/golden")
}

/// Every fixture this module builds, in the order they are written.
#[must_use]
pub fn fixtures() -> Vec<Shape> {
    vec![maximal(), vocal_fx()]
}

/// The project file a fixture is committed as, relative to the fixtures
/// directory.
#[must_use]
pub fn rpp_file(shape: &Shape) -> String {
    format!("{}.rpp", shape.name)
}

/// What one regeneration wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// The project files, relative to the fixtures directory.
    pub projects: Vec<String>,
    /// The media files, relative to the fixtures directory.
    pub media: Vec<String>,
}

/// Regenerate the synthetic media every fixture references under `dir`.
///
/// The media is generated rather than committed, so a checkout has the
/// project files and nothing to play: a test that opens one calls this
/// first. Deterministic and cheap — a second of audio a track — so
/// re-running it is how "make sure it is there" is spelled.
///
/// # Errors
///
/// When a file cannot be written.
pub fn write_media(dir: &Path) -> std::io::Result<Vec<String>> {
    let mut written = Vec::new();
    for shape in fixtures() {
        written.extend(media::write_all(dir, &build(&shape).tracks, &shape)?);
    }
    Ok(written)
}

/// Regenerate every fixture under `dir`: the project files and the
/// media they reference.
///
/// # Errors
///
/// When a file cannot be written.
pub fn write_fixtures(dir: &Path) -> std::io::Result<Written> {
    std::fs::create_dir_all(dir)?;
    let mut written = Written {
        projects: Vec::new(),
        media: Vec::new(),
    };
    for shape in fixtures() {
        let built = build(&shape);
        let file = rpp_file(&shape);
        std::fs::write(dir.join(&file), &built.rpp)?;
        // The song as a keyflow chart beside its project: what the Chart
        // view and a re-stamp read (`session::keyflow::from_chart`).
        if !shape.sections.is_empty() {
            let chart = song::chart(shape.name, shape.bpm, &shape.sections);
            std::fs::write(dir.join(&file).with_extension("kf"), chart)?;
        }
        written.projects.push(file);
        written
            .media
            .extend(media::write_all(dir, &built.tracks, &shape)?);
    }
    Ok(written)
}

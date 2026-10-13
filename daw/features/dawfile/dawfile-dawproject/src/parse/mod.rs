//! Parse orchestration for DawProject files.

pub mod arrangement;
pub mod clips;
pub mod devices;
pub mod tempo;
pub mod tracks;
pub mod xml_helpers;

use crate::error::{DawProjectError, DawProjectResult};
use crate::types::*;
use xml_helpers::*;

/// Parse a DawProject from the decompressed XML of `project.xml`.
pub fn parse_project(xml: &str) -> DawProjectResult<DawProject> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| DawProjectError::Xml(e.to_string()))?;

    let root = doc.root_element();
    if root.tag_name().name() != "Project" {
        return Err(DawProjectError::MissingRoot);
    }

    let version = attr(root, "version").unwrap_or("1.0").to_string();
    if !version.starts_with('1') {
        return Err(DawProjectError::UnsupportedVersion(version));
    }

    let application = child(root, "Application").map(|app| Application {
        name: attr(app, "name").unwrap_or("").to_string(),
        version: attr(app, "version").unwrap_or("").to_string(),
    });

    let transport = child(root, "Transport")
        .map(tempo::parse_transport)
        .unwrap_or_default();

    let track_list = child(root, "Structure")
        .map(tracks::parse_tracks)
        .unwrap_or_default();

    let arrangement = child(root, "Arrangement").map(arrangement::parse_arrangement);

    let scenes = child(root, "Scenes")
        .map(clips::parse_scenes)
        .unwrap_or_default();

    Ok(DawProject {
        version,
        application,
        metadata: None,
        transport,
        tracks: track_list,
        arrangement,
        scenes,
    })
}

/// Parse project metadata from `metadata.xml`.
pub fn parse_metadata(xml: &str) -> DawProjectResult<ProjectMetadata> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| DawProjectError::Xml(e.to_string()))?;

    let root = doc.root_element();
    let mut meta = ProjectMetadata::default();

    let read = |name: &str| -> Option<String> {
        child(root, name)
            .and_then(|n| n.text())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };

    meta.title = read("Title");
    meta.artist = read("Artist");
    meta.album = read("Album");
    meta.composer = read("Composer");
    meta.songwriter = read("Songwriter");
    meta.producer = read("Producer");
    meta.original_artist = read("OriginalArtist");
    meta.arranger = read("Arranger");
    meta.year = read("Year");
    meta.genre = read("Genre");
    meta.copyright = read("Copyright");
    meta.website = read("Website");
    meta.comment = read("Comment");

    Ok(meta)
}

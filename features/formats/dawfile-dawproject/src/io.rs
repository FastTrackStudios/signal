//! File I/O entry points.
//!
//! High-level functions for reading and writing DawProject files (.dawproject).

use crate::error::{DawProjectError, DawProjectResult};
use crate::parse;
use crate::types::DawProject;
use crate::write;
use std::io::{Read, Write};
use std::path::Path;

/// Read and parse a DawProject file (`.dawproject`).
///
/// Extracts `project.xml` (and optionally `metadata.xml`) from the ZIP
/// archive and returns the parsed project.
///
/// # Example
///
/// ```no_run
/// let project = dawfile_dawproject::read_project("project.dawproject")?;
///
/// println!("Version: {}", project.version);
/// println!("Tempo: {:.1} BPM", project.transport.tempo);
/// println!("Tracks: {}", project.tracks.len());
/// # Ok::<(), dawfile_dawproject::DawProjectError>(())
/// ```
pub fn read_project(path: impl AsRef<Path>) -> DawProjectResult<DawProject> {
    let data = std::fs::read(path.as_ref())?;
    parse_project_bytes(&data)
}

/// Parse a DawProject from raw bytes (the raw `.dawproject` ZIP content).
///
/// Useful when the archive is already in memory.
pub fn parse_project_bytes(data: &[u8]) -> DawProjectResult<DawProject> {
    let cursor = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|_| DawProjectError::NotZip)?;

    // Extract project.xml
    let project_xml =
        read_zip_entry(&mut archive, "project.xml").ok_or(DawProjectError::MissingProjectXml)?;
    let project_str = String::from_utf8(project_xml)
        .map_err(|e| DawProjectError::Xml(format!("invalid UTF-8 in project.xml: {e}")))?;

    let mut project = parse::parse_project(&project_str)?;

    // Extract metadata.xml (optional)
    if let Some(meta_xml) = read_zip_entry(&mut archive, "metadata.xml")
        && let Ok(meta_str) = String::from_utf8(meta_xml)
    {
        project.metadata = parse::parse_metadata(&meta_str).ok();
    }

    Ok(project)
}

/// Write a `DawProject` to a `.dawproject` file at `path`.
///
/// Creates (or overwrites) the file with a ZIP archive containing
/// `project.xml` and, if metadata is present, `metadata.xml`.
///
/// # Example
///
/// ```no_run
/// use dawfile_dawproject::{DawProject, Transport, write_project};
///
/// let project = DawProject {
///     version: "1.0".to_string(),
///     application: None,
///     metadata: None,
///     transport: Transport { tempo: 120.0, numerator: 4, denominator: 4 },
///     tracks: vec![],
///     arrangement: None,
///     scenes: vec![],
/// };
///
/// write_project(&project, "output.dawproject")?;
/// # Ok::<(), dawfile_dawproject::DawProjectError>(())
/// ```
pub fn write_project(project: &DawProject, path: impl AsRef<Path>) -> DawProjectResult<()> {
    let bytes = serialize_project_bytes(project)?;
    std::fs::write(path.as_ref(), bytes)?;
    Ok(())
}

/// Serialize a `DawProject` to raw `.dawproject` ZIP bytes in memory.
pub fn serialize_project_bytes(project: &DawProject) -> DawProjectResult<Vec<u8>> {
    let buf = Vec::new();
    let cursor = std::io::Cursor::new(buf);
    let mut zip = zip::ZipWriter::new(cursor);

    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // project.xml
    let project_xml = write::write_project_xml(project);
    zip.start_file("project.xml", options)?;
    zip.write_all(project_xml.as_bytes())?;

    // metadata.xml (optional)
    if let Some(meta) = &project.metadata {
        let meta_xml = write::write_metadata_xml(meta);
        zip.start_file("metadata.xml", options)?;
        zip.write_all(meta_xml.as_bytes())?;
    }

    let cursor = zip.finish()?;
    Ok(cursor.into_inner())
}

/// Read a named entry from a ZIP archive into a `Vec<u8>`.
///
/// Returns `None` if the entry does not exist.
fn read_zip_entry(
    archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
    name: &str,
) -> Option<Vec<u8>> {
    let mut entry = archive.by_name(name).ok()?;
    let mut buf = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut buf).ok()?;
    Some(buf)
}

//! The `.frame` file.
//!
//! A `.frame` bundle is a zip archive (deflate) with three kinds of entry:
//!
//! ```text
//! manifest.styx       Manifest — format version, doc id, title, timestamps,
//!                     the param catalogs the document needs
//! doc.loro            the Loro snapshot of the scene (history included,
//!                     asset *bytes* excluded — see document-loro)
//! assets/<asset id>   raw asset bytes; the name is their blake3 hex, the
//!                     mime lives in the doc's asset metadata
//! ```
//!
//! Asset bytes sit beside the CRDT, not in it: they're immutable and
//! content-addressed, so the doc only needs to name them, and a snapshot
//! stays small however many images a document carries. Loading checks
//! every asset against its name (a flipped bit is an error, not a wrong
//! image) and that every asset the doc references is present.
//!
//! The manifest is written with `facet-styx` from the [`Manifest`] struct.

use std::io::{Cursor, Read, Write};
use std::path::Path;

use chrono::{DateTime, Utc};
use document_loro::{AssetStorage, LoroScene};
use document_proto::{AssetId, Scene};
use facet::Facet;
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// The bundle format this crate writes (and the only one it reads).
pub const FORMAT_VERSION: u32 = 1;

pub const MANIFEST: &str = "manifest.styx";
pub const DOC: &str = "doc.loro";
pub const ASSETS_DIR: &str = "assets/";

/// A bundle's `manifest.styx`: what the file is, before opening the doc.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Manifest {
    /// The bundle format ([`FORMAT_VERSION`]).
    pub format: u32,
    /// The document's identity — the same id its sync channel and index
    /// row (`document_proto::Document`) use.
    pub doc_id: Uuid,
    pub title: String,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    /// Param catalogs the document's bindings need, by name.
    pub catalogs: Vec<String>,
}

impl Manifest {
    /// A manifest for a new document, created now.
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        let now = now_seconds();
        Self {
            format: FORMAT_VERSION,
            doc_id: Uuid::new_v4(),
            title: title.into(),
            created: now,
            updated: now,
            catalogs: Vec::new(),
        }
    }

    /// Mark the document as updated now.
    pub fn touch(&mut self) {
        self.updated = now_seconds();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("manifest: {0}")]
    Manifest(String),
    #[error("unsupported bundle format {0} (this build reads {FORMAT_VERSION})")]
    UnsupportedFormat(u32),
    #[error("doc.loro: {0}")]
    Doc(String),
    #[error("asset {0} is missing")]
    MissingAsset(AssetId),
    #[error("asset {0} doesn't match its content hash")]
    CorruptAsset(AssetId),
}

/// What [`save`] / [`to_bytes`] accept: a live replica (its history is
/// kept) or a plain scene (written as a fresh doc).
pub trait Content {
    /// Call `f` with a replica whose snapshot carries no asset bytes.
    fn with_live<R>(&self, f: impl FnOnce(&LoroScene) -> R) -> R;
}

impl Content for LoroScene {
    fn with_live<R>(&self, f: impl FnOnce(&LoroScene) -> R) -> R {
        match self.storage() {
            AssetStorage::External => f(self),
            // Inline bytes would ride inside doc.loro (and its history, for
            // good): re-base onto an external-storage replica instead.
            AssetStorage::Inline => f(&LoroScene::from_scene(&self.scene())),
        }
    }
}

impl Content for Scene {
    fn with_live<R>(&self, f: impl FnOnce(&LoroScene) -> R) -> R {
        f(&LoroScene::from_scene(self))
    }
}

/// Write a bundle to `path`.
///
/// # Errors
/// I/O, or [`Error::MissingAsset`] when the replica references an asset it
/// holds no bytes for.
pub fn save(path: impl AsRef<Path>, content: &impl Content, manifest: &Manifest) -> Result<(), Error> {
    let bytes = to_bytes(content, manifest)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Read a bundle from `path`.
///
/// # Errors
/// See [`from_bytes`].
pub fn load(path: impl AsRef<Path>) -> Result<(Manifest, LoroScene), Error> {
    from_bytes(&std::fs::read(path)?)
}

/// A bundle, in memory.
///
/// # Errors
/// See [`save`].
pub fn to_bytes(content: &impl Content, manifest: &Manifest) -> Result<Vec<u8>, Error> {
    content.with_live(|live| {
        let scene = live.scene();
        if let Some(missing) = live.missing_assets().into_iter().next() {
            return Err(Error::MissingAsset(missing));
        }
        let manifest_text = facet_styx::to_string(manifest).map_err(|e| Error::Manifest(e.to_string()))?;

        let deflate = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file(MANIFEST, deflate)?;
        zip.write_all(manifest_text.as_bytes())?;
        zip.start_file(DOC, deflate)?;
        zip.write_all(&live.export_snapshot())?;
        for asset in scene.assets.values() {
            zip.start_file(format!("{ASSETS_DIR}{}", asset.id), deflate)?;
            zip.write_all(&asset.bytes)?;
        }
        Ok(zip.finish()?.into_inner())
    })
}

/// Open an in-memory bundle: the manifest, and a replica of its doc with
/// every asset's bytes loaded and verified.
///
/// # Errors
/// Not a zip / missing entries, an unreadable manifest, a newer format,
/// a doc Loro can't import, an asset whose bytes don't hash to its name
/// ([`Error::CorruptAsset`]), or one the doc references that isn't there
/// ([`Error::MissingAsset`]).
pub fn from_bytes(bytes: &[u8]) -> Result<(Manifest, LoroScene), Error> {
    let mut zip = ZipArchive::new(Cursor::new(bytes))?;

    let manifest_text = String::from_utf8(read_entry(&mut zip, MANIFEST)?)
        .map_err(|e| Error::Manifest(e.to_string()))?;
    let manifest: Manifest = facet_styx::from_str(&manifest_text).map_err(|e| Error::Manifest(e.to_string()))?;
    if manifest.format != FORMAT_VERSION {
        return Err(Error::UnsupportedFormat(manifest.format));
    }

    let doc = read_entry(&mut zip, DOC)?;
    let live = LoroScene::from_snapshot(&doc, AssetStorage::External).map_err(|e| Error::Doc(e.to_string()))?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(name) = entry.name().strip_prefix(ASSETS_DIR).map(str::to_owned) else { continue };
        if name.is_empty() {
            continue;
        }
        let mut data = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
        entry.read_to_end(&mut data)?;
        let id = AssetId(name);
        if AssetId::of(&data) != id {
            return Err(Error::CorruptAsset(id));
        }
        live.put_blob(data);
    }
    if let Some(missing) = live.missing_assets().into_iter().next() {
        return Err(Error::MissingAsset(missing));
    }
    Ok((manifest, live))
}

fn read_entry(zip: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Vec<u8>, Error> {
    let mut entry = zip.by_name(name)?;
    let mut out = Vec::new();
    entry.read_to_end(&mut out)?;
    Ok(out)
}

/// Now, at whole-second precision: manifests round-trip through styx, which
/// stores timestamps to the second.
fn now_seconds() -> chrono::DateTime<Utc> {
    use chrono::SubsecRound as _;
    Utc::now().trunc_subsecs(0)
}

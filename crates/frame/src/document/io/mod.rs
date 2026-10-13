//! Surfaces on disk: the one load/save API every host uses.
//!
//! A surface is frame markup (`.frame`; the older `.fm` is still read and
//! written where it already exists) or a bundle (also `.frame`: one zip with
//! manifest, Loro snapshot and assets — see `document-bundle`). The content
//! decides which a `.frame` is: a zip (`PK\x03\x04`) is a bundle, anything
//! else is markup. Markup names
//! its image and font assets by id only, so their bytes live in a sidecar
//! folder next to it:
//!
//! ```text
//! 06-ranch-head.frame                 the markup (text, diffable, editable)
//! 06-ranch-head.frame.assets/
//!   manifest.json                     { "format": 1, "assets": [ { "id", "mime", "file" } ] }
//!   9f2c…e1.png                       asset bytes; the name is their blake3 hex
//!   41ab…07.ttf
//! ```
//!
//! Why a sidecar rather than base64 in the markup or bundles only:
//!
//! - the `.frame` stays the small, human- and agent-editable text it was — a
//!   model reading or rewriting a face never wades through megabytes of
//!   base64, and a diff of a face shows design changes, not re-encoded PNGs;
//! - asset files are content-addressed: a name *is* the hash of the bytes,
//!   so writes are idempotent, loading verifies every file, and identical
//!   textures (a suite's shared tolex) are identical files — git stores one
//!   blob for all of them;
//! - one folder per surface keeps a surface self-contained: copying,
//!   moving or deleting it is `file + file.assets/`, no shared store to
//!   garbage-collect or race on;
//! - the manifest keeps each asset's mime (fonts are found by `font/*`,
//!   not by file extension) and says which files are the document's.
//!
//! [`load_surface`] / [`save_surface`] pick the format by extension. The
//! lower-level [`read_markup`] / [`write_markup`] move markup *text* plus
//! assets without parsing (a library serves a half-edited file as is).
//! [`fingerprint`] covers the file and its sidecar, so a watcher that polls
//! it reloads when a texture changes as well as when the markup does.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash as _, Hasher as _};
use std::path::{Path, PathBuf};

use crate::document::proto::{Asset, AssetId, Scene};
use serde::{Deserialize, Serialize};

/// The extension of a frame document (markup).
pub const EXTENSION: &str = "frame";
/// The extension frame markup used before it was `.frame`; still read.
pub const LEGACY_EXTENSION: &str = "fm";

/// Whether `path` names a frame document: `.frame` (markup or a bundle) or
/// the legacy `.fm`.
#[must_use]
pub fn is_frame_document(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e, EXTENSION | LEGACY_EXTENSION))
}

/// Appended to a markup file's name for its assets folder (`a.frame.assets`).
pub const ASSETS_SUFFIX: &str = ".assets";
/// The sidecar's index of assets.
pub const MANIFEST: &str = "manifest.json";
/// The sidecar format this crate writes (and the newest it reads).
pub const MANIFEST_FORMAT: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// Displays as `path:line:col: message`.
    #[error("{}:{error}", path.display())]
    Parse {
        path: PathBuf,
        error: crate::document::text::ParseError,
    },
    #[error("{}: {error}", path.display())]
    Bundle {
        path: PathBuf,
        error: crate::document::bundle::Error,
    },
    #[error("{}: {message}", path.display())]
    Manifest { path: PathBuf, message: String },
    #[error("{}: asset {id} is missing", path.display())]
    MissingAsset { path: PathBuf, id: AssetId },
    #[error("{}: asset {id} doesn't match its content hash", path.display())]
    CorruptAsset { path: PathBuf, id: AssetId },
    #[error("{}: not a surface file (.frame or .fm)", path.display())]
    UnknownFormat { path: PathBuf },
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.to_owned(),
        source,
    }
}

/// A surface file's format. Markup and bundles share the `.frame`
/// extension; the content decides (see [`Format::of`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// `.frame` (or legacy `.fm`) markup text (+ its `.frame.assets/` sidecar).
    Markup,
    /// A `.frame` bundle: one zip.
    Bundle,
}

impl Format {
    /// The format of the surface file at `path`. A `.frame` that exists is
    /// a bundle when it starts with the zip magic, markup otherwise; a
    /// `.frame` not written yet is markup (see [`save_surface_as`] to pick).
    /// `.fm` is always markup.
    #[must_use]
    pub fn of(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()? {
            LEGACY_EXTENSION => Some(Self::Markup),
            EXTENSION => Some(if is_zip(path) {
                Self::Bundle
            } else {
                Self::Markup
            }),
            _ => None,
        }
    }
}

/// The zip local-file header every bundle starts with.
const ZIP_MAGIC: &[u8; 4] = b"PK\x03\x04";

/// Whether `bytes` is a bundle (a zip) rather than markup text.
#[must_use]
pub fn is_bundle_bytes(bytes: &[u8]) -> bool {
    bytes.starts_with(ZIP_MAGIC)
}

/// Whether the file at `path` starts with a zip local-file header.
fn is_zip(path: &Path) -> bool {
    use std::io::Read as _;
    let mut magic = [0u8; 4];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && is_bundle_bytes(&magic)
}

/// Markup text and the assets it names, as stored.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Markup {
    pub text: String,
    pub assets: BTreeMap<AssetId, Asset>,
}

/// Load a surface: a bundle, or `.frame` / `.fm` markup with its assets.
///
/// # Errors
/// Unreadable file, a markup parse error (`path:line:col: message`), a
/// broken bundle, or a sidecar asset that's missing or corrupt.
pub fn load_surface(path: impl AsRef<Path>) -> Result<Scene, Error> {
    let path = path.as_ref();
    match Format::of(path) {
        Some(Format::Bundle) => crate::document::bundle::load(path)
            .map(|(_, doc)| doc.scene())
            .map_err(|error| Error::Bundle {
                path: path.to_owned(),
                error,
            }),
        Some(Format::Markup) => {
            let Markup { text, assets } = read_markup(path)?;
            crate::document::text::parse(&text, assets).map_err(|error| Error::Parse {
                path: path.to_owned(),
                error,
            })
        }
        None => Err(Error::UnknownFormat {
            path: path.to_owned(),
        }),
    }
}

/// Save a surface in the format [`Format::of`] gives `path`: `.fm`, a new
/// `.frame` or an existing text `.frame` writes the markup plus its assets
/// sidecar (removed when the scene has no assets); an existing bundle
/// `.frame` writes a bundle, keeping its identity (doc id, created). To
/// write a bundle to a new path, use [`save_surface_as`].
///
/// Each file is written then renamed into place, assets before the markup,
/// so a watcher never sees markup naming assets that aren't there yet.
///
/// # Errors
/// I/O, an unknown extension, or a bundle that can't be written.
pub fn save_surface(path: impl AsRef<Path>, scene: &Scene) -> Result<(), Error> {
    let path = path.as_ref();
    let format = Format::of(path).ok_or_else(|| Error::UnknownFormat {
        path: path.to_owned(),
    })?;
    save_surface_as(path, scene, format)
}

/// Save a surface to `path` (a `.frame` or `.fm`) in an explicit `format`.
/// A bundle replaces a markup file's assets sidecar; markup replaces a
/// bundle's bytes. Same atomicity as [`save_surface`].
///
/// # Errors
/// I/O, an unknown extension, or a bundle that can't be written.
pub fn save_surface_as(path: impl AsRef<Path>, scene: &Scene, format: Format) -> Result<(), Error> {
    let path = path.as_ref();
    if !is_frame_document(path) {
        return Err(Error::UnknownFormat {
            path: path.to_owned(),
        });
    }
    match format {
        Format::Markup => write_markup(
            path,
            &crate::document::text::print(scene),
            scene.assets.values(),
        ),
        Format::Bundle => {
            let bundle = |error| Error::Bundle {
                path: path.to_owned(),
                error,
            };
            let manifest = match crate::document::bundle::load(path) {
                Ok((mut m, _)) => {
                    m.touch();
                    m
                }
                Err(_) => crate::document::bundle::Manifest::new(title_of(scene, path)),
            };
            let bytes = crate::document::bundle::to_bytes(scene, &manifest).map_err(bundle)?;
            write_atomic(path, &bytes)?;
            prune_assets(path)
        }
    }
}

/// The first top-level node's name, else the file stem.
fn title_of(scene: &Scene, path: &Path) -> String {
    scene
        .pages()
        .first()
        .and_then(|p| scene.node(*p))
        .and_then(|p| p.children.first())
        .and_then(|n| scene.node(*n))
        .map(|n| n.name.trim().to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
}

/// Read markup text and its sidecar assets, without parsing the text.
///
/// # Errors
/// Unreadable / non-UTF-8 file, or a broken sidecar (see [`read_assets`]).
pub fn read_markup(path: impl AsRef<Path>) -> Result<Markup, Error> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(io(path))?;
    Ok(Markup {
        text,
        assets: read_assets(path)?,
    })
}

/// Write markup text and its assets: the sidecar's new files and manifest
/// first, then the text, then the sidecar's stale files are pruned.
///
/// # Errors
/// I/O.
pub fn write_markup<'a>(
    path: impl AsRef<Path>,
    text: &str,
    assets: impl IntoIterator<Item = &'a Asset>,
) -> Result<(), Error> {
    let path = path.as_ref();
    let assets: Vec<&Asset> = assets.into_iter().collect();
    let keep = put_assets(path, &assets)?;
    write_atomic(path, text.as_bytes())?;
    prune(path, &keep)
}

/// The assets in `path`'s sidecar (none when it has no sidecar). Every
/// file is checked against its content hash.
///
/// # Errors
/// An unreadable or newer-format manifest, a listed file that's missing
/// ([`Error::MissingAsset`]) or whose bytes don't hash to its id
/// ([`Error::CorruptAsset`]).
pub fn read_assets(path: impl AsRef<Path>) -> Result<BTreeMap<AssetId, Asset>, Error> {
    let dir = assets_dir(path.as_ref());
    let manifest_path = dir.join(MANIFEST);
    let text = match std::fs::read_to_string(&manifest_path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(io(&manifest_path)(e)),
    };
    let bad = |message: String| Error::Manifest {
        path: manifest_path.clone(),
        message,
    };
    let manifest: Manifest = serde_json::from_str(&text).map_err(|e| bad(e.to_string()))?;
    if manifest.format > MANIFEST_FORMAT {
        return Err(bad(format!(
            "sidecar format {} (this build reads {MANIFEST_FORMAT})",
            manifest.format
        )));
    }
    let mut out = BTreeMap::new();
    for entry in manifest.assets {
        if !plain_name(&entry.file) {
            return Err(bad(format!(
                "`{}` isn't a file name in the assets folder",
                entry.file
            )));
        }
        let file = dir.join(&entry.file);
        let bytes = match std::fs::read(&file) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::MissingAsset {
                    path: file,
                    id: entry.id,
                });
            }
            Err(e) => return Err(io(&file)(e)),
        };
        if AssetId::of(&bytes) != entry.id {
            return Err(Error::CorruptAsset {
                path: file,
                id: entry.id,
            });
        }
        out.insert(
            entry.id.clone(),
            Asset {
                id: entry.id,
                mime: entry.mime,
                bytes,
            },
        );
    }
    Ok(out)
}

/// Remove a surface file and (for markup) its assets sidecar.
///
/// # Errors
/// I/O other than the file already being gone.
pub fn remove_surface(path: impl AsRef<Path>) -> Result<(), Error> {
    let path = path.as_ref();
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io(path)(e)),
    }
    prune(path, &BTreeSet::new())
}

/// `path`'s assets folder: `a.frame` → `a.frame.assets`.
#[must_use]
pub fn assets_dir(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(ASSETS_SUFFIX);
    PathBuf::from(name)
}

/// Whether `dir` is some markup file's assets folder (for directory walks
/// that should skip them).
#[must_use]
pub fn is_assets_dir(dir: &Path) -> bool {
    dir.file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_suffix(ASSETS_SUFFIX))
        .is_some_and(|stem| Format::of(Path::new(stem)) == Some(Format::Markup))
}

/// A cheap identity of the surface on disk — the file's and every sidecar
/// entry's name, mtime and length — for change polling. `None` when the
/// file doesn't exist. Changes when the markup changes *or* an asset does.
#[must_use]
pub fn fingerprint(path: impl AsRef<Path>) -> Option<u64> {
    let path = path.as_ref();
    let stamp = |p: &Path| {
        std::fs::metadata(p)
            .ok()
            .map(|m| (m.modified().ok(), m.len()))
    };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    stamp(path)?.hash(&mut h);
    if Format::of(path) == Some(Format::Markup)
        && let Ok(read) = std::fs::read_dir(assets_dir(path))
    {
        let mut names: Vec<PathBuf> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
        names.sort();
        for p in names {
            p.file_name().hash(&mut h);
            stamp(&p).hash(&mut h);
        }
    }
    Some(h.finish())
}

/// The file extension an asset of `mime` is stored with (cosmetic: the
/// manifest, not the extension, is the asset's mime).
#[must_use]
pub fn extension_for(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "font/ttf" => "ttf",
        "font/otf" => "otf",
        "font/woff" => "woff",
        "font/woff2" => "woff2",
        "text/wgsl" => "wgsl",
        _ => "bin",
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    format: u32,
    assets: Vec<ManifestEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ManifestEntry {
    id: AssetId,
    mime: String,
    file: String,
}

fn file_name(asset: &Asset) -> String {
    format!("{}.{}", asset.id, extension_for(&asset.mime))
}

/// Write `assets`' files (unless already there) and the manifest (unless
/// unchanged). Returns the file names the sidecar now holds.
fn put_assets(path: &Path, assets: &[&Asset]) -> Result<BTreeSet<String>, Error> {
    if assets.is_empty() {
        return Ok(BTreeSet::new());
    }
    let dir = assets_dir(path);
    std::fs::create_dir_all(&dir).map_err(io(&dir))?;
    let mut sorted = assets.to_vec();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    sorted.dedup_by(|a, b| a.id == b.id);
    let mut keep = BTreeSet::new();
    let mut entries = Vec::with_capacity(sorted.len());
    for asset in sorted {
        let name = file_name(asset);
        let file = dir.join(&name);
        // Content-addressed: a file of that name holding those bytes is done.
        if std::fs::read(&file).ok().as_deref() != Some(asset.bytes.as_slice()) {
            write_atomic(&file, &asset.bytes)?;
        }
        entries.push(ManifestEntry {
            id: asset.id.clone(),
            mime: asset.mime.clone(),
            file: name.clone(),
        });
        keep.insert(name);
    }
    let manifest = Manifest {
        format: MANIFEST_FORMAT,
        assets: entries,
    };
    let mut text = serde_json::to_string_pretty(&manifest).map_err(|e| Error::Manifest {
        path: dir.join(MANIFEST),
        message: e.to_string(),
    })?;
    text.push('\n');
    let manifest_path = dir.join(MANIFEST);
    if std::fs::read_to_string(&manifest_path).ok().as_deref() != Some(text.as_str()) {
        write_atomic(&manifest_path, text.as_bytes())?;
    }
    keep.insert(MANIFEST.to_owned());
    Ok(keep)
}

/// Remove the sidecar's asset files not in `keep` (and the manifest and
/// folder too when `keep` is empty). Only files named like assets are
/// touched: anything else someone put there stays.
/// Remove the assets sidecar of `path` (a bundle carries its own assets, so
/// one left over from when the file was markup is stale).
///
/// # Errors
/// I/O other than the files already being gone.
pub fn prune_assets(path: &Path) -> Result<(), Error> {
    prune(path, &BTreeSet::new())
}

fn prune(path: &Path, keep: &BTreeSet<String>) -> Result<(), Error> {
    let dir = assets_dir(path);
    let Ok(read) = std::fs::read_dir(&dir) else {
        return Ok(());
    };
    for entry in read.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if keep.contains(&name) {
            continue;
        }
        let ours = name == MANIFEST || name.split('.').next().is_some_and(is_asset_id);
        if ours {
            let p = entry.path();
            match std::fs::remove_file(&p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io(&p)(e)),
            }
        }
    }
    if keep.is_empty() {
        // Fails (and stays) when someone else's files are in it.
        let _ = std::fs::remove_dir(&dir);
    }
    Ok(())
}

/// A blake3 hex id.
fn is_asset_id(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn plain_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.') && !name.contains(['/', '\\'])
}

/// Write-then-rename, so a watcher (or any reader) never sees half a file.
/// The temp name ends in `.tmp`, which nothing scans as a surface.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(io(dir))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes).map_err(io(&tmp))?;
    std::fs::rename(&tmp, path).map_err(io(path))
}

#[cfg(test)]
mod tests;

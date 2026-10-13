//! REAPER theme importer.
//!
//! Loads an **unpacked** REAPER theme (the `.ReaperTheme` ini + its same-named
//! image folder holding `rtconfig.txt` and the PNG atlases — see
//! `reaper-theme/docs/theme-images.md`) into typed data the FTS UI can map
//! onto its own theme model:
//!
//! - [`Palette`] — the `[color theme]` section (Windows `COLORREF` ints).
//! - [`RtConfig`] — `rtconfig.txt` global directives + `define_parameter`s.
//! - [`ImageCatalog`] — the PNG vocabulary, with slicers for the REAPER atlas
//!   geometries (3-slice buttons, `mcp_vu` meter slices, pink/yellow-marked
//!   backgrounds).
//!
//! This crate is renderer-agnostic on purpose: it produces colors, scalars and
//! sliced RGBA images. The mapping into `daw-ui`'s `Theme`/`McpColors`/image
//! skin lives behind a `daw-ui` feature so the parser stays UI-free.

pub mod images;
pub mod palette;
pub mod rtconfig;
pub mod walter;

pub use images::{ImageCatalog, Slice3};
pub use palette::{Drawmode, Palette, Rgba};
pub use rtconfig::{RtConfig, ThemeParamDef};

/// Re-export of the image crate (consumers compose slices without adding the
/// dependency themselves).
pub use image;

use std::path::{Path, PathBuf};

/// Importer errors.
#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    #[error("io error reading {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("no .ReaperTheme file found in {0}")]
    MissingReaperTheme(PathBuf),
    #[error("no image folder (with rtconfig.txt) found next to {0}")]
    MissingImageFolder(PathBuf),
    #[error("image error in {path}: {source}")]
    Image {
        path: PathBuf,
        source: image::ImageError,
    },
    #[error("image {path} too small for {geometry} slicing ({width}x{height})")]
    BadGeometry {
        path: PathBuf,
        geometry: &'static str,
        width: u32,
        height: u32,
    },
}

/// A loaded, unpacked REAPER theme.
pub struct ReaperTheme {
    /// Theme root: the directory holding the `.ReaperTheme` file.
    pub root: PathBuf,
    /// The `[color theme]` palette.
    pub palette: Palette,
    /// `rtconfig.txt` globals + `define_parameter`s.
    pub rtconfig: RtConfig,
    /// The raw `rtconfig.txt` source — the WALTER program
    /// ([`walter::evaluate`] runs it per environment).
    pub rtconfig_src: String,
    /// The image folder's PNG vocabulary (lazy decode).
    pub images: ImageCatalog,
}

impl ReaperTheme {
    /// Build a theme from raw sources — the `.ReaperTheme` ini text and the
    /// `rtconfig.txt` text — with **no image catalog**. For embedding a theme
    /// into a binary on platforms without a filesystem (wasm): palette,
    /// rtconfig globals/params, and the WALTER program all parse and evaluate
    /// from the strings; image skins fall back to vector (see
    /// [`ImageCatalog::empty`]).
    pub fn from_sources(reaper_theme_ini: &str, rtconfig: &str) -> Self {
        Self {
            root: PathBuf::new(),
            palette: Palette::parse(reaper_theme_ini),
            rtconfig: RtConfig::parse(rtconfig),
            rtconfig_src: rtconfig.to_string(),
            images: ImageCatalog::empty(),
        }
    }

    /// Load from a directory containing `X.ReaperTheme` + `X/` (or the image
    /// folder itself if the ini sits next to it under any name).
    pub fn load_dir(dir: impl AsRef<Path>) -> Result<Self, ThemeError> {
        let dir = dir.as_ref();
        let read_dir = |p: &Path| {
            std::fs::read_dir(p).map_err(|source| ThemeError::Io {
                path: p.to_path_buf(),
                source,
            })
        };

        // Find the .ReaperTheme ini.
        let theme_file = read_dir(dir)?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("ReaperTheme"))
            })
            .ok_or_else(|| ThemeError::MissingReaperTheme(dir.to_path_buf()))?;

        // Find the image folder: prefer the ini's stem, else any subdir with
        // an rtconfig.txt.
        let stem_dir = theme_file.with_extension("");
        let image_dir = if stem_dir.join("rtconfig.txt").is_file() {
            stem_dir
        } else {
            read_dir(dir)?
                .flatten()
                .map(|e| e.path())
                .find(|p| p.is_dir() && p.join("rtconfig.txt").is_file())
                .ok_or_else(|| ThemeError::MissingImageFolder(theme_file.clone()))?
        };

        let ini = std::fs::read_to_string(&theme_file).map_err(|source| ThemeError::Io {
            path: theme_file.clone(),
            source,
        })?;
        let rt_path = image_dir.join("rtconfig.txt");
        let rt = std::fs::read_to_string(&rt_path).map_err(|source| ThemeError::Io {
            path: rt_path,
            source,
        })?;

        Ok(Self {
            root: dir.to_path_buf(),
            palette: Palette::parse(&ini),
            rtconfig: RtConfig::parse(&rt),
            rtconfig_src: rt,
            images: ImageCatalog::scan(&image_dir)?,
        })
    }
}

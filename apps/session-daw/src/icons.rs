//! The toolbar icons REAPER already has, in the rails.
//!
//! `MixPhase::icon` names a file, and `fts-icons` renders and installs
//! it: one PNG holding three cells side by side — normal, hover,
//! clicked — which is the same `Interaction` the rest of this window
//! draws with. So an icon is not new art; it is art this project
//! already ships, read from where it was installed.
//!
//! # Why the words survive
//!
//! Every icon is optional at every step: the resource path may not
//! exist, the file may not have been installed, the PNG may not decode.
//! A rail that failed to find an icon prints its label, which is what
//! it did before this module and is legible either way. An icon is an
//! improvement on a label, never a requirement for one.
//!
//! # Why they are cached as three images
//!
//! The strip is one image and the cell is a third of it. Cropping with
//! a brush transform would need `Extend::Pad` and would smear the
//! neighbouring cell along the edge at any non-integer scale; slicing
//! at decode time costs three allocations once and cannot smear.

use std::collections::HashMap;
use std::path::PathBuf;

use daw_theme_art::mixer_controls::Interaction;
use vello::peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat};

/// Where REAPER keeps its resources on this machine.
///
/// The same two candidates `fts-icons` installs into, plus an env
/// override for a checkout that is neither. A dev tree is checked
/// first: someone running this window from the repo is the person most
/// likely to have just rebuilt the icons into it.
const ENV: &str = "FTS_REAPER_RESOURCE_PATH";

/// The decoded icons, kept for the life of the window.
///
/// A cache of `Option` rather than a cache of successes: a missing icon
/// must be looked for once, not once per frame, and "this file is not
/// there" is exactly as worth remembering as the file.
#[derive(Debug, Default)]
pub struct Icons {
    root: Option<PathBuf>,
    cache: HashMap<String, Option<[ImageBrush; 3]>>,
}

impl Icons {
    /// Find the resource path. Cheap — one or two `is_file` calls.
    #[must_use]
    pub fn new() -> Self {
        Self {
            root: resource_path().map(|path| path.join("Data").join("toolbar_icons")),
            cache: HashMap::new(),
        }
    }

    /// The cell for `name` in a given state, if there is one.
    ///
    /// `&mut self` because the first ask decodes. That is the whole
    /// reason this is a struct rather than a function: a rail redrawn
    /// at 240 fps must not hit the filesystem.
    pub fn cell(&mut self, name: &str, at: Interaction) -> Option<&ImageBrush> {
        if !self.cache.contains_key(name) {
            let loaded = self.root.as_ref().and_then(|root| {
                let mut file = root.join(name);
                file.set_extension("png");
                strip(&file)
            });
            self.cache.insert(name.to_owned(), loaded);
        }
        let cells = self.cache.get(name)?.as_ref()?;
        cells.get(match at {
            Interaction::Normal => 0,
            Interaction::Hover => 1,
            Interaction::Pressed => 2,
        })
    }

    /// No icons at all, whatever is installed on this machine.
    ///
    /// For the measurement shots: the reference every viewport in the
    /// sweep is compared against must not depend on whether the person
    /// running it happens to have REAPER's resources on disk.
    #[must_use]
    pub fn none() -> Self {
        Self {
            root: None,
            cache: HashMap::new(),
        }
    }

    /// Whether any icon could be found at all.
    ///
    /// Worth asking once: a window with no resource path should not
    /// try, and the answer decides whether the rails lay out for icons
    /// or for words.
    #[must_use]
    pub const fn available(&self) -> bool {
        self.root.is_some()
    }
}

/// The REAPER resource directory — the one holding `reaper.ini`.
fn resource_path() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(from_env) = std::env::var_os(ENV) {
        candidates.push(PathBuf::from(from_env));
    }
    #[cfg(feature = "native")]
    {
        if let Some(home) = dirs::home_dir() {
            candidates.push(home.join("fts-dev"));
            candidates.push(home.join(".fts-dev"));
        }
        if let Some(config) = dirs::config_dir() {
            candidates.push(config.join("REAPER"));
        }
    }
    candidates
        .into_iter()
        .find(|path| path.join("reaper.ini").is_file())
}

/// Decode a three-cell strip into three images.
///
/// `None` for anything that is not one: a file that is not there, a
/// PNG that does not decode, or a width that is not divisible by three
/// — which would mean the strip is not the shape REAPER reads either,
/// and guessing at it would put a third of one state beside two thirds
/// of another.
fn strip(file: &std::path::Path) -> Option<[ImageBrush; 3]> {
    let decoded = image::open(file).ok()?.into_rgba8();
    let (width, height) = decoded.dimensions();
    if width == 0 || height == 0 || width % 3 != 0 {
        return None;
    }
    let cell_w = width / 3;
    let cells: Vec<ImageBrush> = (0..3)
        .map(|i| {
            let mut pixels = Vec::with_capacity(
                (cell_w as usize)
                    .saturating_mul(height as usize)
                    .saturating_mul(4),
            );
            for y in 0..height {
                for x in 0..cell_w {
                    let pixel = decoded.get_pixel(i * cell_w + x, y);
                    pixels.extend_from_slice(&pixel.0);
                }
            }
            ImageBrush::new(ImageData {
                data: Blob::new(std::sync::Arc::new(pixels)),
                format: ImageFormat::Rgba8,
                // The PNG's alpha is straight, not premultiplied.
                alpha_type: ImageAlphaType::Alpha,
                width: cell_w,
                height,
            })
        })
        .collect();
    cells.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A missing icon is a `None`, not a panic and not a blank image:
    /// the rail has to be able to fall back to its label.
    #[test]
    fn a_missing_icon_is_absent() {
        let mut icons = Icons {
            root: Some(PathBuf::from("/nonesuch")),
            cache: HashMap::new(),
        };
        assert!(icons.cell("fts_mix_tone", Interaction::Normal).is_none());
        // And the miss is remembered, so the filesystem is asked once.
        assert!(icons.cache.contains_key("fts_mix_tone"));
    }

    /// A window with no REAPER at all says so rather than looking.
    #[test]
    fn no_resource_path_means_no_icons() {
        let mut icons = Icons {
            root: None,
            cache: HashMap::new(),
        };
        assert!(!icons.available());
        assert!(icons.cell("fts_mix_tone", Interaction::Normal).is_none());
    }

    /// A strip that is not three cells wide is not a strip. Guessing
    /// would put a third of one state beside two thirds of another.
    #[test]
    fn a_strip_must_divide_by_three() {
        let dir = std::env::temp_dir().join("fts-icon-strip-test");
        std::fs::create_dir_all(&dir).expect("a temp dir");
        let file = dir.join("odd.png");
        image::RgbaImage::new(7, 3)
            .save(&file)
            .expect("write a png");
        assert!(strip(&file).is_none());

        let three = dir.join("three.png");
        image::RgbaImage::new(9, 3)
            .save(&three)
            .expect("write a png");
        let cells = strip(&three).expect("a three-cell strip");
        assert!(cells.iter().all(|cell| cell.image.width == 3));
    }
}

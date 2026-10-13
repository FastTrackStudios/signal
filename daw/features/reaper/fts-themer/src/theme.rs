//! An unpacked theme on disk, and the edits we make to it.
//!
//! A REAPER theme directory is a `<name>.ReaperTheme` ini beside a `<name>/`
//! folder holding `rtconfig.txt` and the PNGs. The folder name is not a
//! convention we can assume — it's whatever `ui_img=` in the ini says — so
//! [`ThemeDir::open`] resolves it through the ini rather than by guessing.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

use crate::color::Rgb;
use crate::ini::ThemeIni;
use crate::recolor::{Retint, dominant_accent};

/// The DPI variants REAPER looks for: (image subfolder, scale factor).
/// The 100% set lives at the image-folder root, hence the empty prefix.
pub const SCALES: [(&str, f32); 3] = [("", 1.0), ("150", 1.5), ("200", 2.0)];

/// The one image an accent variant folder overrides — the mixer fader cap.
pub const ACCENT_IMAGE: &str = "mcp_volthumb.png";

/// An unpacked theme.
#[derive(Clone, Debug)]
pub struct ThemeDir {
    /// Directory holding the `.ReaperTheme` and the image folder.
    pub root: PathBuf,
    /// Theme name — the `.ReaperTheme` stem and the image folder name.
    pub name: String,
    ini: ThemeIni,
}

impl ThemeDir {
    /// Open the theme rooted at `root`.
    ///
    /// `root` may be the directory containing the pair, or the `.ReaperTheme`
    /// file itself.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        let (dir, ini_path) = if root.extension().is_some_and(|e| e == "ReaperTheme") {
            (
                root.parent().unwrap_or(Path::new(".")).to_path_buf(),
                root.to_path_buf(),
            )
        } else {
            (root.to_path_buf(), find_ini(root)?)
        };

        let ini = ThemeIni::load(&ini_path)?;
        // The image folder is whatever ui_img says; fall back to the ini stem.
        let name = ini
            .reaper_value("ui_img")
            .map(str::to_string)
            .or_else(|| {
                ini_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(str::to_string)
            })
            .context("theme has no ui_img and no usable file name")?;

        let theme = Self {
            root: dir,
            name,
            ini,
        };
        if !theme.images_dir().is_dir() {
            bail!(
                "theme image folder {} does not exist (ui_img points at {:?})",
                theme.images_dir().display(),
                theme.name
            );
        }
        Ok(theme)
    }

    /// Path to the `.ReaperTheme` ini.
    pub fn ini_path(&self) -> PathBuf {
        self.root.join(format!("{}.ReaperTheme", self.name))
    }

    /// The image folder (`<root>/<name>`).
    pub fn images_dir(&self) -> PathBuf {
        self.root.join(&self.name)
    }

    /// Path to `rtconfig.txt`.
    pub fn rtconfig_path(&self) -> PathBuf {
        self.images_dir().join("rtconfig.txt")
    }

    /// The parsed color ini.
    pub fn ini(&self) -> &ThemeIni {
        &self.ini
    }

    /// Mutable access, for editing colors.
    pub fn ini_mut(&mut self) -> &mut ThemeIni {
        &mut self.ini
    }

    /// Write the color ini back to disk.
    pub fn save_ini(&self) -> Result<()> {
        self.ini.save(self.ini_path())
    }

    /// The image folder for a DPI variant (`""` → root, `"150"` → `150/`).
    pub fn scale_dir(&self, scale_prefix: &str) -> PathBuf {
        if scale_prefix.is_empty() {
            self.images_dir()
        } else {
            self.images_dir().join(scale_prefix)
        }
    }

    /// Accent variants this theme offers, per `rtconfig.txt`.
    ///
    /// Read from the `A_Fader_*` layouts rather than the filesystem: a
    /// subfolder holding the accent image is not necessarily an accent —
    /// `strip/` overrides the fader cap among ten other images and is a
    /// layout variant. Folders whose artwork is missing are dropped, so the
    /// list is what REAPER can actually render.
    pub fn accents(&self) -> Result<Vec<String>> {
        let path = self.rtconfig_path();
        let text =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        Ok(crate::rtconfig::accent_folders(&text)
            .into_iter()
            .filter(|f| self.images_dir().join(f).join(ACCENT_IMAGE).is_file())
            .collect())
    }

    /// Generate an accent variant: recolor [`ACCENT_IMAGE`] at all three DPI
    /// scales into `<images>/<name>/`, `150/<name>/`, `200/<name>/`.
    ///
    /// `source` names an existing accent folder to recolor from. Its own
    /// dominant color is detected and used as the remap source, so the result
    /// lands on `color` exactly regardless of which variant you copied.
    ///
    /// Returns the files written.
    pub fn generate_accent(
        &self,
        name: &str,
        color: Rgb,
        source: &str,
        keep_lightness: bool,
    ) -> Result<Vec<PathBuf>> {
        if name.is_empty() || name.contains(['/', '\\']) {
            bail!("invalid accent name {name:?}");
        }
        let mut written = Vec::new();

        // Detect the source color once, from the 100% art (the most pixels).
        let base = self.scale_dir("").join(source).join(ACCENT_IMAGE);
        if !base.is_file() {
            bail!(
                "source accent {source:?} has no {ACCENT_IMAGE} at {}",
                base.display()
            );
        }
        let base_img = image::open(&base)
            .with_context(|| format!("read {}", base.display()))?
            .to_rgba8();
        let from = dominant_accent(&base_img)
            .with_context(|| format!("{} has no saturated pixels to recolor", base.display()))?;
        let retint = if keep_lightness {
            Retint::new(from, color).keeping_lightness()
        } else {
            Retint::new(from, color)
        };

        for (prefix, _) in SCALES {
            let src = self.scale_dir(prefix).join(source).join(ACCENT_IMAGE);
            if !src.is_file() {
                // A theme may ship only some DPI variants; skip rather than fail.
                continue;
            }
            let dst = self.scale_dir(prefix).join(name).join(ACCENT_IMAGE);
            retint.apply_file(&src, &dst)?;
            written.push(dst);
        }

        if written.is_empty() {
            bail!("no {ACCENT_IMAGE} found under accent {source:?} at any scale");
        }
        Ok(written)
    }
}

/// Find the single `.ReaperTheme` in a directory.
fn find_ini(dir: &Path) -> Result<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("read {}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "ReaperTheme"))
        .collect();
    found.sort();
    match found.len() {
        0 => bail!("no .ReaperTheme in {}", dir.display()),
        1 => Ok(found.remove(0)),
        _ => bail!(
            "{} holds {} .ReaperTheme files — point at one explicitly",
            dir.display(),
            found.len()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    /// A minimal theme on disk: ini + image folder + one blue accent.
    fn fixture(dir: &Path, ui_img: &str) {
        std::fs::create_dir_all(dir.join(ui_img).join("blue")).unwrap();
        std::fs::create_dir_all(dir.join(ui_img).join("150").join("blue")).unwrap();
        std::fs::write(
            dir.join(format!("{ui_img}.ReaperTheme")),
            format!("[color theme]\ncol_main_bg=1118481\n\n[REAPER]\nui_img={ui_img}\n"),
        )
        .unwrap();
        std::fs::write(
            dir.join(ui_img).join("rtconfig.txt"),
            "version 6.0\n\
             Layout \"A\"\n\
             \tLayout \"A_Fader_Blue\" \"blue\"\n\
             \t\tset mcp.label .\n\
             \tEndLayout\n\
             EndLayout\n\
             Layout \"A_Strip\" \"strip\"\n\
             EndLayout\n",
        )
        .unwrap();
        // A layout variant that also overrides the fader cap — must not be
        // mistaken for an accent.
        std::fs::create_dir_all(dir.join(ui_img).join("strip")).unwrap();

        // A shaded blue cap over grey chrome.
        let thumb = RgbaImage::from_fn(4, 1, |x, _| match x {
            0 => Rgba([0x45, 0x45, 0x45, 255]),
            1 => Rgba([0x46, 0xb9, 0xfe, 255]),
            2 => Rgba([0x2f, 0x8a, 0xc4, 255]),
            _ => Rgba([0, 0, 0, 0]),
        });
        thumb
            .save(dir.join(ui_img).join("blue").join(ACCENT_IMAGE))
            .unwrap();
        thumb
            .save(dir.join(ui_img).join("strip").join(ACCENT_IMAGE))
            .unwrap();
        thumb
            .save(dir.join(ui_img).join("150").join("blue").join(ACCENT_IMAGE))
            .unwrap();
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fts-themer-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn image_folder_comes_from_ui_img_not_the_file_name() {
        let d = tmpdir("uiimg");
        // Folder deliberately differs from the .ReaperTheme stem's usual case.
        fixture(&d, "Renamed");
        std::fs::rename(
            d.join("Renamed.ReaperTheme"),
            d.join("Something Else.ReaperTheme"),
        )
        .unwrap();

        let theme = ThemeDir::open(&d).unwrap();
        assert_eq!(theme.name, "Renamed");
        assert!(theme.images_dir().ends_with("Renamed"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn accents_excludes_dpi_folders_and_layout_variants() {
        let d = tmpdir("accents");
        fixture(&d, "T");
        let theme = ThemeDir::open(&d).unwrap();
        // 150/ mirrors blue/ but is a DPI set; strip/ carries the fader cap
        // but is a layout variant. Only the registered accent counts.
        assert_eq!(theme.accents().unwrap(), vec!["blue"]);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn generating_artwork_alone_does_not_register_an_accent() {
        // The two halves are separable, and half-done must not look done:
        // images with no layout are invisible to REAPER.
        let d = tmpdir("halfdone");
        fixture(&d, "T");
        let theme = ThemeDir::open(&d).unwrap();
        theme
            .generate_accent("crimson", Rgb::new(0xd1, 0x28, 0x3c), "blue", false)
            .unwrap();
        assert_eq!(theme.accents().unwrap(), vec!["blue"]);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn generate_accent_writes_every_available_scale() {
        let d = tmpdir("gen");
        fixture(&d, "T");
        let theme = ThemeDir::open(&d).unwrap();
        let written = theme
            .generate_accent("crimson", Rgb::new(0xd1, 0x28, 0x3c), "blue", false)
            .unwrap();

        // 100% and 150% exist in the fixture; 200% does not and is skipped.
        assert_eq!(written.len(), 2);
        assert!(
            theme
                .scale_dir("")
                .join("crimson")
                .join(ACCENT_IMAGE)
                .is_file()
        );
        assert!(
            theme
                .scale_dir("150")
                .join("crimson")
                .join(ACCENT_IMAGE)
                .is_file()
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn generated_accent_hits_the_requested_colour_and_spares_grey() {
        let d = tmpdir("colour");
        fixture(&d, "T");
        let theme = ThemeDir::open(&d).unwrap();
        let target = Rgb::new(0xd1, 0x28, 0x3c);
        theme
            .generate_accent("crimson", target, "blue", false)
            .unwrap();

        let out = image::open(theme.scale_dir("").join("crimson").join(ACCENT_IMAGE))
            .unwrap()
            .to_rgba8();
        // Grey chrome untouched.
        assert_eq!(out.get_pixel(0, 0).0, [0x45, 0x45, 0x45, 255]);
        // Transparent pixel still transparent.
        assert_eq!(out.get_pixel(3, 0).0[3], 0);
        // The body pixel now reads as the target hue.
        let body = Rgb::new(
            out.get_pixel(1, 0).0[0],
            out.get_pixel(1, 0).0[1],
            out.get_pixel(1, 0).0[2],
        );
        let (h, _, _) = body.hsl();
        let (want, _, _) = target.hsl();
        let dist = (h - want).abs().min(360.0 - (h - want).abs());
        assert!(dist < 12.0, "body hue {h} not near target {want}");
        // The darker shade stays darker — the ramp survived.
        let shade = Rgb::new(
            out.get_pixel(2, 0).0[0],
            out.get_pixel(2, 0).0[1],
            out.get_pixel(2, 0).0[2],
        );
        assert!(shade.luminance() < body.luminance());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn generate_accent_rejects_a_missing_source() {
        let d = tmpdir("nosrc");
        fixture(&d, "T");
        let theme = ThemeDir::open(&d).unwrap();
        assert!(
            theme
                .generate_accent("x", Rgb::new(1, 2, 3), "does-not-exist", false)
                .is_err()
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn generate_accent_rejects_a_path_traversing_name() {
        let d = tmpdir("traverse");
        fixture(&d, "T");
        let theme = ThemeDir::open(&d).unwrap();
        assert!(
            theme
                .generate_accent("../escape", Rgb::new(1, 2, 3), "blue", false)
                .is_err()
        );
        std::fs::remove_dir_all(&d).ok();
    }
}

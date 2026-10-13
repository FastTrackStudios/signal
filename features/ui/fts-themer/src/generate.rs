//! Writing component-drawn artwork into a REAPER theme.
//!
//! `restyle` recolours the *inherited* art; this replaces it. Each image is
//! rendered from its Dioxus component at 100/150/200 % — from the vector
//! each time, not upscaled — and written over the corresponding PNG.
//!
//! Generated images are the only ones that don't need a pristine snapshot:
//! they have no prior state to preserve, because their source is code.

use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::theme::{SCALES, ThemeDir};

/// What [`generate`] wrote.
#[derive(Debug, Clone, Default)]
pub struct GenerateReport {
    pub written: Vec<PathBuf>,
    /// Of `written`, those drawn from a true vector control rather than
    /// from traced rects — worth reporting because only these get sharper
    /// at 150/200%, and a control silently falling back to its trace is
    /// invisible in the output.
    pub vectorised: Vec<PathBuf>,
    pub failed: Vec<(String, String)>,
}

/// Render every component-drawn image into `theme`.
///
/// Each image's geometry is **measured from the art being replaced** — the
/// pristine snapshot `restyle` keeps, falling back to what's live. Nothing
/// about size or marker layout is authored here, because REAPER blits these
/// where WALTER expects and only reads magenta as geometry when it lands
/// where it expects. Getting that wrong renders the guides as visible
/// magenta in the mixer.
pub fn generate(theme: &ThemeDir, dry_run: bool, vectors_only: bool) -> Result<GenerateReport> {
    let mut report = GenerateReport::default();

    for (prefix, _scale) in SCALES {
        let dir = theme.scale_dir(prefix);
        if !dir.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "png") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            // The traced art is keyed on the 100% names; a DPI variant
            // finds the same drawing and renders it at the size that
            // folder wants.
            let Some(art) = daw_theme_art::generated::by_name(name) else {
                continue;
            };

            // Geometry still comes from the image being replaced, not from
            // the traced source: REAPER's 150% art is not reliably 1.5x the
            // 100%, and blitting at the wrong size is what renders markers
            // as visible magenta.
            //
            // From the **pristine snapshot** where there is one, because
            // measuring the live file makes generation self-referential:
            // once a run split a strip wrongly and wrote a stretched
            // button over it, every later run measured the stretched
            // button, agreed it was a single cell, and reproduced the
            // mistake. That looks exactly like a fix not taking effect.
            let measured_from = pristine(&dir, name).unwrap_or_else(|| path.clone());
            let measured = match image::open(&measured_from) {
                Ok(img) => daw_theme_art::DerivedSpec::from_image(&img.to_rgba8()),
                Err(e) => {
                    report.failed.push((name.into(), format!("read: {e}")));
                    continue;
                }
            };

            // A vector control if one draws this image, the trace
            // otherwise. Both stamp the same measured markers back.
            let vector = daw_theme_art::cell_markup(name, Default::default()).is_some();
            // The traced half rewrites art nothing here authored, through
            // a ramp built from the theme's own palette. Skipping it is
            // the default; see the `--traced` flag for why.
            if vectors_only && !vector {
                continue;
            }
            let drawn = if vector {
                daw_theme_art::render_control(name, &measured).map_err(|e| format!("{e}"))
            } else {
                render_art(art, &measured)
            };

            match drawn {
                Ok(img) => {
                    if !dry_run
                        && let Err(e) = img
                            .save(&path)
                            .with_context(|| format!("write {}", path.display()))
                    {
                        report.failed.push((name.into(), format!("{e:#}")));
                        continue;
                    }
                    if vector {
                        report.vectorised.push(path.clone());
                    }
                    report.written.push(path);
                }
                Err(e) => report.failed.push((name.into(), e)),
            }
        }
    }
    report.written.sort();
    report.vectorised.sort();
    Ok(report)
}

/// The untouched original of `name`, if the theme kept one.
///
/// `restyle` snapshots every image into `.source-art/` before its first
/// pass, so this is what the theme shipped rather than what the last run
/// left behind.
fn pristine(dir: &std::path::Path, name: &str) -> Option<PathBuf> {
    let p = dir
        .join(crate::restyle::SOURCE_DIR)
        .join(format!("{name}.png"));
    p.is_file().then_some(p)
}

/// Render traced art at the geometry measured from the image it replaces,
/// stamping that image's marker pixels back.
///
/// Cell by cell, because `ArtImage` draws **one** sprite cell — asking it
/// for the full strip width does not draw the strip, it stretches cell 0
/// across all of it. That produces a plausible-looking blurry button
/// rather than an obviously wrong one, so it is worth being explicit
/// about here.
fn render_art(
    art: daw_theme_art::ArtData,
    spec: &daw_theme_art::DerivedSpec,
) -> std::result::Result<image::RgbaImage, String> {
    use daw_theme_art::art_data::{ArtImage, ArtImageProps, ColorMode};

    // Split by the *trace's* cell count, not the measured one.
    //
    // `ArtImage` shifts its viewBox by `cell * (art.width / art.cells)`,
    // so it can only address the cells the trace knows about. When the two
    // disagreed — the index built with an older detector, the spec
    // measured with the current one — every cell index clamped to 0 and
    // the whole drawing was composited three times side by side, which is
    // what put three copies of a toolbar icon inside one icon.
    let mut spec = spec.clone();
    let n = art.cells.max(1);
    if spec.cells.len() as u32 != n {
        let w = spec.width / n;
        spec.cells = (0..n).map(|i| (i * w, w)).collect();
    }

    daw_theme_art::composite_cells(&spec, |i, w| {
        Ok(daw_theme_art::render_svg(
            ArtImage,
            ArtImageProps {
                art,
                width: Some(w),
                height: Some(spec.height),
                mode: ColorMode::Themed,
                cell: i,
            },
        ))
    })
    .map_err(|e| format!("{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};
    use std::path::Path;

    /// A theme whose `mcp_bg` matches REAPER's real shape: 4x4 with two
    /// marker pixels at opposite corners. 100% and 150% exist; 200% does not.
    fn fixture(dir: &Path) {
        std::fs::create_dir_all(dir.join("T").join("150")).unwrap();
        std::fs::write(
            dir.join("T.ReaperTheme"),
            "[color theme]\ncol_main_bg=0\n\n[REAPER]\nui_img=T\n",
        )
        .unwrap();
        std::fs::write(dir.join("T").join("rtconfig.txt"), "version 6.0\n").unwrap();

        let art = RgbaImage::from_fn(4, 4, |x, y| match (x, y) {
            (0, 0) | (3, 3) => Rgba([255, 0, 255, 255]),
            _ => Rgba([0x2a, 0x2a, 0x2a, 255]),
        });
        art.save(dir.join("T").join("mcp_bg.png")).unwrap();
        art.save(dir.join("T").join("150").join("mcp_bg.png"))
            .unwrap();
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fts-gen-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn writes_only_the_scales_and_names_the_theme_has() {
        let d = tmpdir("scales");
        fixture(&d);
        let theme = ThemeDir::open(&d).unwrap();
        let report = generate(&theme, false, false).unwrap();
        assert!(report.failed.is_empty(), "{:?}", report.failed);

        // mcp_bg at two scales. mcp_volbg is in the registry but absent from
        // this theme, so it is skipped rather than invented.
        assert_eq!(report.written.len(), 2, "{:?}", report.written);
        assert!(!d.join("T").join("mcp_volbg.png").exists());
        assert!(!d.join("T").join("200").exists(), "invented a 200/ folder");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_generated_image_keeps_the_sources_geometry() {
        // The failure this replaced: authoring a size produced art REAPER
        // blits wrongly and magenta it draws as visible pixels.
        let d = tmpdir("geometry");
        fixture(&d);
        let theme = ThemeDir::open(&d).unwrap();
        generate(&theme, false, false).unwrap();

        let img = image::open(d.join("T").join("mcp_bg.png"))
            .unwrap()
            .to_rgba8();
        assert_eq!(img.dimensions(), (4, 4), "size drifted from the source");
        // Markers restored exactly where they were, and nowhere else.
        assert_eq!(img.get_pixel(0, 0).0, [255, 0, 255, 255]);
        assert_eq!(img.get_pixel(3, 3).0, [255, 0, 255, 255]);
        assert_ne!(img.get_pixel(3, 0).0, [255, 0, 255, 255]);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_art_between_the_markers_is_actually_redrawn() {
        let d = tmpdir("redraw");
        fixture(&d);
        let theme = ThemeDir::open(&d).unwrap();
        generate(&theme, false, false).unwrap();
        let img = image::open(d.join("T").join("mcp_bg.png"))
            .unwrap()
            .to_rgba8();
        assert_ne!(
            img.get_pixel(1, 1).0,
            [0x2a, 0x2a, 0x2a, 255],
            "component did not replace the source art"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn dry_run_writes_nothing() {
        let d = tmpdir("dry");
        fixture(&d);
        let theme = ThemeDir::open(&d).unwrap();
        let before = std::fs::read(d.join("T").join("mcp_bg.png")).unwrap();
        let report = generate(&theme, true, false).unwrap();
        assert_eq!(report.written.len(), 2);
        assert_eq!(
            std::fs::read(d.join("T").join("mcp_bg.png")).unwrap(),
            before
        );
        std::fs::remove_dir_all(&d).ok();
    }
}

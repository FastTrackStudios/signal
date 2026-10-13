//! Restyling a theme's artwork onto the palette.
//!
//! The palette reaches everything REAPER draws itself. It does not reach
//! the PNGs — toolbar backgrounds, mixer strips, TCP frames, button faces —
//! which is why a fully-mapped palette can still leave the mixer looking
//! exactly like the theme it was forked from.
//!
//! This walks the image folder and pushes every neutral pixel through
//! [`daw_theme::Ramp`]. Bevels, gradients and antialiasing survive because
//! only *absolute* lightness moves; relative lightness is preserved.
//!
//! Three kinds of pixel are never touched:
//!
//! - **WALTER marker pixels** (magenta / yellow) — nine-slice geometry.
//! - **Fully transparent pixels** — recolouring them can fringe on scaling.
//! - **Saturated pixels** — LEDs, record buttons, fader caps. Their colour
//!   is their meaning.

use anyhow::{Context, Result};
use image::Rgba;
use std::path::{Path, PathBuf};

use crate::color::Rgb;
use crate::recolor::is_marker;
use crate::theme::ThemeDir;

/// What [`restyle`] did.
#[derive(Debug, Clone, Default)]
pub struct RestyleReport {
    /// Images whose pixels actually changed.
    pub changed: Vec<PathBuf>,
    /// Images visited but left byte-identical.
    pub unchanged: usize,
    /// Images that could not be read or written.
    pub failed: Vec<(PathBuf, String)>,
}

/// Where pristine artwork is kept, relative to the image folder.
///
/// A luminance remap is **not idempotent** — it compounds on its own
/// output, so restyling twice darkens twice and there is no way back to
/// try a different palette. Every run therefore maps from an untouched
/// snapshot rather than from whatever is currently on disk, taken once on
/// the first run.
pub const SOURCE_DIR: &str = ".source-art";

/// Push every neutral pixel of every PNG under the theme through `ramp`.
///
/// The first call snapshots the current artwork into [`SOURCE_DIR`]; that
/// snapshot is the input from then on, so a palette change can be re-run
/// as many times as you like and always produces the same result.
///
/// `dry_run` reports without writing — worth having, because this rewrites
/// a few thousand files at once.
pub fn restyle(theme: &ThemeDir, ramp: &daw_theme::Ramp, dry_run: bool) -> Result<RestyleReport> {
    let mut report = RestyleReport::default();
    let root = theme.images_dir();
    let source = root.join(SOURCE_DIR);

    if !source.is_dir() {
        if dry_run {
            // Nothing to compare against yet, so a dry run would report the
            // whole theme as changing whatever the palette says. Say so
            // rather than print a misleading number.
            anyhow::bail!(
                "no pristine snapshot yet — run without --dry-run once to create {}",
                source.display()
            );
        }
        snapshot(&root, &source)
            .with_context(|| format!("snapshot artwork into {}", source.display()))?;
    }

    let mut pending = vec![source.clone()];

    while let Some(dir) = pending.pop() {
        let entries = std::fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "png") {
                continue;
            }
            // Mirror the snapshot's layout back into the live folder.
            let Ok(rel) = path.strip_prefix(&source) else {
                continue;
            };
            let dest = root.join(rel);
            match restyle_one(&path, &dest, ramp, dry_run) {
                Ok(true) => report.changed.push(dest),
                Ok(false) => report.unchanged += 1,
                Err(e) => report.failed.push((dest, format!("{e:#}"))),
            }
        }
    }

    report.changed.sort();
    Ok(report)
}

/// Copy every PNG under `root` into `dest`, preserving layout.
fn snapshot(root: &Path, dest: &Path) -> Result<()> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Don't snapshot the snapshot.
                if path.file_name().is_some_and(|n| n == SOURCE_DIR) {
                    continue;
                }
                pending.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "png") {
                continue;
            }
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let target = dest.join(rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

/// Restyle one PNG from `src` into `dest`. Returns whether anything changed.
fn restyle_one(src: &Path, dest: &Path, ramp: &daw_theme::Ramp, dry_run: bool) -> Result<bool> {
    let mut img = image::open(src)
        .with_context(|| format!("read {}", src.display()))?
        .to_rgba8();

    for Rgba([r, g, b, alpha]) in img.pixels_mut() {
        if *alpha == 0 {
            continue;
        }
        if is_marker(Rgb::new(*r, *g, *b)) {
            continue;
        }
        let mapped = ramp.apply(daw_theme::Color::rgb(*r, *g, *b));
        (*r, *g, *b) = (mapped.r, mapped.g, mapped.b);
    }

    // "Changed" means *the destination is not already this*, not "the ramp
    // moved the source". Comparing against the source would report every
    // image as changed on every run, since the source is pristine by
    // design — which would make the dry run useless and hide whether a
    // palette edit actually did anything.
    let changed = match image::open(dest) {
        Ok(existing) => existing.to_rgba8() != img,
        Err(_) => true,
    };

    if changed && !dry_run {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        img.save(dest)
            .with_context(|| format!("write {}", dest.display()))?;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    fn fixture(dir: &Path) {
        std::fs::create_dir_all(dir.join("T").join("150")).unwrap();
        std::fs::write(
            dir.join("T.ReaperTheme"),
            "[color theme]\ncol_main_bg=0\n\n[REAPER]\nui_img=T\n",
        )
        .unwrap();
        std::fs::write(dir.join("T").join("rtconfig.txt"), "version 6.0\n").unwrap();

        // Grey chrome, a WALTER marker, a saturated LED, a clear pixel.
        let art = RgbaImage::from_fn(4, 1, |x, _| match x {
            0 => Rgba([0x3e, 0x3e, 0x3e, 255]),
            1 => Rgba([255, 0, 255, 255]),
            2 => Rgba([0xe1, 0x3a, 0x53, 255]),
            _ => Rgba([0x3e, 0x3e, 0x3e, 0]),
        });
        art.save(dir.join("T").join("mcp_bg.png")).unwrap();
        // A nested scale folder, to prove the walk recurses.
        art.save(dir.join("T").join("150").join("mcp_bg.png"))
            .unwrap();
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("fts-restyle-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn ramp() -> daw_theme::Ramp {
        daw_theme::Ramp::for_chrome(&daw_theme::Theme::default())
    }

    #[test]
    fn walks_every_scale_folder() {
        let d = tmpdir("walk");
        fixture(&d);
        let theme = ThemeDir::open(&d).unwrap();
        let report = restyle(&theme, &ramp(), false).unwrap();
        assert_eq!(report.changed.len(), 2, "did not recurse into 150/");
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn markers_leds_and_transparency_survive() {
        let d = tmpdir("protect");
        fixture(&d);
        let theme = ThemeDir::open(&d).unwrap();
        restyle(&theme, &ramp(), false).unwrap();

        let out = image::open(d.join("T").join("mcp_bg.png"))
            .unwrap()
            .to_rgba8();
        // Chrome moved...
        assert_ne!(out.get_pixel(0, 0).0[..3], [0x3e, 0x3e, 0x3e]);
        // ...the nine-slice marker did not (it is geometry, not art)...
        assert_eq!(out.get_pixel(1, 0).0, [255, 0, 255, 255]);
        // ...the LED kept its meaning...
        assert_eq!(out.get_pixel(2, 0).0, [0xe1, 0x3a, 0x53, 255]);
        // ...and a clear pixel kept its RGB, so scaling can't fringe.
        assert_eq!(out.get_pixel(3, 0).0, [0x3e, 0x3e, 0x3e, 0]);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn dry_run_reports_without_writing() {
        let d = tmpdir("dry");
        fixture(&d);
        let theme = ThemeDir::open(&d).unwrap();
        // A dry run needs the snapshot to compare against.
        restyle(&theme, &ramp(), false).unwrap();
        let before = std::fs::read(d.join("T").join("mcp_bg.png")).unwrap();
        let report = restyle(&theme, &ramp(), true).unwrap();
        assert!(
            report.changed.is_empty(),
            "already restyled: {:?}",
            report.changed
        );
        assert_eq!(
            std::fs::read(d.join("T").join("mcp_bg.png")).unwrap(),
            before
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn restyling_twice_changes_nothing_the_second_time() {
        // Idempotence matters: the ramp must be a fixed point on its own
        // output, or every run drifts the art further.
        let d = tmpdir("idem");
        fixture(&d);
        let theme = ThemeDir::open(&d).unwrap();
        restyle(&theme, &ramp(), false).unwrap();
        let second = restyle(&theme, &ramp(), false).unwrap();
        assert!(
            second.changed.is_empty(),
            "second pass moved {:?}",
            second.changed
        );
        std::fs::remove_dir_all(&d).ok();
    }
}

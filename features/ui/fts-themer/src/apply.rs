//! Painting a REAPER theme from the canonical FTS theme.
//!
//! [`daw_theme::Theme`] is the authored source; this writes the keys it
//! determines into a `.ReaperTheme`. Everything else in the file — the keys
//! the palette has no opinion about, the blend words, the font blobs — is
//! left exactly as it was, so applying is a **merge onto** a theme rather
//! than a replacement of one. That is what lets an FTS palette be applied on
//! top of a hand-tuned theme like the Reapertips fork without flattening the
//! parts nobody has modelled yet.

use anyhow::{Context, Result};
use std::path::Path;

use crate::color::Rgb;
use crate::theme::ThemeDir;

/// What [`apply_theme`] changed.
#[derive(Debug, Clone, Default)]
pub struct ApplyReport {
    /// Keys whose value actually changed.
    pub changed: Vec<(String, Rgb, Rgb)>,
    /// Keys the theme determined that already held that colour.
    pub unchanged: usize,
    /// Where the SWELL theme was written, if anywhere.
    pub swell: Option<std::path::PathBuf>,
    /// Overridden keys this REAPER theme does not have.
    ///
    /// Worth surfacing rather than writing: a key REAPER never reads is a
    /// typo that looks exactly like a colour that "didn't take".
    pub unknown: Vec<String>,
    /// Overridden keys that are not colours — `*_drawmode` and friends.
    ///
    /// Writing a colour over a blend mode changes how a layer composites,
    /// which shows up as a rendering bug a long way from its cause.
    pub not_a_colour: Vec<String>,
}

impl ApplyReport {
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty()
    }
}

/// Write `theme`'s palette into the REAPER theme at `dir`.
///
/// `dry_run` computes the diff without touching the file — worth having,
/// because this rewrites a file the user may have hand-edited.
pub fn apply_theme(dir: &Path, theme: &daw_theme::Theme, dry_run: bool) -> Result<ApplyReport> {
    apply_theme_to(dir, theme, dry_run, None)
}

/// As [`apply_theme`], also writing `libSwell.colortheme` into
/// `resource_dir` when one is given.
///
/// SWELL's theme is a separate file in REAPER's *resource* directory, not
/// in the theme directory, and it paints the menu bar, dialogs, buttons and
/// list controls — none of which the `.ReaperTheme` palette can reach.
/// Skipping it is why a "finished" dark theme keeps a grey bar across the
/// top and grey dialogs throughout.
pub fn apply_theme_to(
    dir: &Path,
    theme: &daw_theme::Theme,
    dry_run: bool,
    resource_dir: Option<&Path>,
) -> Result<ApplyReport> {
    let mut target = ThemeDir::open(dir)?;
    let mut report = ApplyReport::default();

    for assignment in theme.reaper_palette() {
        // Guarded only for overrides, and deliberately so.
        //
        // A *derived* key the theme happens not to define is normal — a
        // theme sets what it cares about and REAPER defaults the rest, so
        // adding `envcp_bg` to a theme that omits it is the feature, not a
        // mistake. An *override* naming an unknown key is almost always a
        // typo, and REAPER ignores it silently, so it looks exactly like a
        // colour that refused to take effect. Same for a `*_drawmode`:
        // writing a colour over a blend mode changes how a layer
        // composites and surfaces a long way from the cause.
        if theme.overrides.contains_key(assignment.key.as_ref()) {
            if !target.ini().has(&assignment.key) {
                report.unknown.push(assignment.key.to_string());
                continue;
            }
            if !crate::groups::is_color(&assignment.key) {
                report.not_a_colour.push(assignment.key.to_string());
                continue;
            }
        }
        // daw-theme and fts-themer each have their own colour type — one is
        // the authoring vocabulary, the other the REAPER wire format — so
        // convert rather than leaking one into the other's API.
        let want = Rgb::new(assignment.color.r, assignment.color.g, assignment.color.b);
        match target.ini().color(&assignment.key) {
            Some(have) if have == want => report.unchanged += 1,
            have => {
                report.changed.push((
                    assignment.key.to_string(),
                    have.unwrap_or(Rgb::new(0, 0, 0)),
                    want,
                ));
                target.ini_mut().set_color(&assignment.key, want);
            }
        }
    }

    if !dry_run && !report.is_empty() {
        target
            .save_ini()
            .with_context(|| format!("write {}", target.ini_path().display()))?;
    }

    if let Some(res) = resource_dir {
        let path = res.join("libSwell.colortheme");
        if !dry_run {
            std::fs::write(&path, theme.swell_colortheme())
                .with_context(|| format!("write {}", path.display()))?;
        }
        report.swell = Some(path);
    }

    Ok(report)
}

/// Load a canonical theme from a styx file, or the built-in default.
pub fn load_theme(path: Option<&Path>) -> Result<daw_theme::Theme> {
    match path {
        Some(p) => {
            let text =
                std::fs::read_to_string(p).with_context(|| format!("read {}", p.display()))?;
            daw_theme::Theme::from_styx(&text)
                .map_err(|e| anyhow::anyhow!("parse {}: {e}", p.display()))
        }
        None => Ok(daw_theme::Theme::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn fixture(dir: &Path) {
        std::fs::create_dir_all(dir.join("T").join("blue")).unwrap();
        std::fs::write(
            dir.join("T.ReaperTheme"),
            // col_main_bg2 already correct-ish; col_main_text deliberately wrong.
            "[color theme]\ncol_main_text=0\nmarquee_drawmode=168\n\n[REAPER]\nui_img=T\ntl_font=KEEPME\n",
        )
        .unwrap();
        std::fs::write(dir.join("T").join("rtconfig.txt"), "version 6.0\n").unwrap();
        RgbaImage::from_fn(1, 1, |_, _| Rgba([0, 0, 0, 0]))
            .save(dir.join("T").join("blue").join("mcp_volthumb.png"))
            .unwrap();
    }

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("fts-apply-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn writes_the_themes_colours_and_leaves_the_rest_alone() {
        let d = tmpdir("write");
        fixture(&d);
        let theme = daw_theme::Theme::default();
        let report = apply_theme(&d, &theme, false).unwrap();
        assert!(!report.is_empty());

        let text = std::fs::read_to_string(d.join("T.ReaperTheme")).unwrap();
        // The font blob and the blend word are untouched.
        assert!(text.contains("tl_font=KEEPME"));
        assert!(text.contains("marquee_drawmode=168"));

        // And the authored text colour actually landed.
        let ini = crate::ThemeIni::parse(&text);
        let want = theme.chrome.text;
        assert_eq!(
            ini.color("col_main_text"),
            Some(Rgb::new(want.r, want.g, want.b))
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn dry_run_reports_without_writing() {
        let d = tmpdir("dry");
        fixture(&d);
        let before = std::fs::read_to_string(d.join("T.ReaperTheme")).unwrap();
        let report = apply_theme(&d, &daw_theme::Theme::default(), true).unwrap();
        assert!(!report.is_empty());
        assert_eq!(
            std::fs::read_to_string(d.join("T.ReaperTheme")).unwrap(),
            before
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn applying_twice_is_a_no_op_the_second_time() {
        // Idempotence matters: this runs in a save loop, and a second run
        // reporting changes would mean the write and the read disagree.
        let d = tmpdir("idem");
        fixture(&d);
        let theme = daw_theme::Theme::default();
        apply_theme(&d, &theme, false).unwrap();
        let second = apply_theme(&d, &theme, false).unwrap();
        assert!(
            second.is_empty(),
            "second apply still changed {:?}",
            second.changed
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn default_theme_is_used_when_no_file_given() {
        assert_eq!(load_theme(None).unwrap(), daw_theme::Theme::default());
    }

    #[test]
    fn a_styx_theme_round_trips_through_load() {
        let d = tmpdir("styx");
        let mut theme = daw_theme::Theme::default();
        theme.name = "Roundtrip".into();
        theme.chrome.accent = daw_theme::Color::rgb(1, 2, 3);
        let path = d.join("t.styx");
        std::fs::write(&path, theme.to_styx().unwrap()).unwrap();
        assert_eq!(load_theme(Some(&path)).unwrap(), theme);
        std::fs::remove_dir_all(&d).ok();
    }
}

#[cfg(test)]
mod override_tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("fts-apply-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("T.ReaperTheme"),
            "[color theme]\ncol_arrangebg=0\nmcp_bg_drawmode=1\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("T")).unwrap();
        dir
    }

    /// The whole point: an exact key reaches the ini.
    #[test]
    fn an_override_reaches_the_theme_file() {
        let dir = scratch("ok");
        let mut theme = daw_theme::Theme::default();
        theme.overrides.insert(
            "col_arrangebg".into(),
            daw_theme::Color::rgb(0x42, 0x42, 0x42),
        );

        apply_theme(&dir, &theme, false).unwrap();
        let written = ThemeDir::open(&dir).unwrap();
        assert_eq!(
            written.ini().color("col_arrangebg"),
            Some(Rgb::new(0x42, 0x42, 0x42)),
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A typo is reported rather than written. REAPER ignores keys it does
    /// not know, so writing one looks exactly like a colour that refused to
    /// take effect — the failure has to surface here or nowhere.
    #[test]
    fn a_key_this_theme_lacks_is_reported_not_written() {
        let dir = scratch("unknown");
        let mut theme = daw_theme::Theme::default();
        theme
            .overrides
            .insert("col_arrangeblag".into(), daw_theme::Color::rgb(1, 2, 3));

        let report = apply_theme(&dir, &theme, false).unwrap();
        assert!(
            report.unknown.iter().any(|k| k == "col_arrangeblag"),
            "not reported: {:?}",
            report.unknown,
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// And a blend mode is refused: writing a colour over one changes how a
    /// layer composites, which shows up as a rendering bug far from here.
    #[test]
    fn a_non_colour_key_is_refused() {
        let dir = scratch("mode");
        let mut theme = daw_theme::Theme::default();
        theme
            .overrides
            .insert("mcp_bg_drawmode".into(), daw_theme::Color::rgb(1, 2, 3));

        let report = apply_theme(&dir, &theme, false).unwrap();
        assert!(
            report.not_a_colour.iter().any(|k| k == "mcp_bg_drawmode"),
            "not reported: {:?}",
            report.not_a_colour,
        );
        let written = ThemeDir::open(&dir).unwrap();
        assert_eq!(written.ini().int("mcp_bg_drawmode"), Some(1), "was written");
        std::fs::remove_dir_all(&dir).ok();
    }
}

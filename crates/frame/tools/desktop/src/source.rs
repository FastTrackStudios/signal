//! Where the surface comes from, and noticing when it changes on disk.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use frame::surface::proto::Catalog;
use frame::surface::runtime::Surface;
use frame::surface::widgets::{Options, Theme, surface_for_catalog};

#[derive(Debug, Clone)]
pub enum Source {
    /// The surface generated from the engine's catalog (the demo engine's,
    /// or a live engine's with `--engine`).
    Generated { theme: Box<Theme> },
    /// frame markup text (`.frame`, or the legacy `.fm`, with its `.frame.assets/` textures and fonts).
    Markup(PathBuf),
    /// A `.frame` bundle (a zip; told from markup by content).
    Bundle(PathBuf),
}

impl Source {
    #[must_use]
    pub fn from_path(path: PathBuf) -> Self {
        if frame::document::io::Format::of(&path) == Some(frame::document::io::Format::Bundle) {
            Self::Bundle(path)
        } else {
            Self::Markup(path)
        }
    }

    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Generated { .. } => None,
            Self::Markup(p) | Self::Bundle(p) => Some(p),
        }
    }

    /// A load error with the path shortened to the file name, for the
    /// in-window banner (stderr keeps the full, clickable path).
    #[must_use]
    pub fn short_error(&self, error: &str) -> String {
        let Some(path) = self.path() else {
            return error.to_owned();
        };
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        error
            .strip_prefix(&*path.display().to_string())
            .map_or_else(|| error.to_owned(), |rest| format!("{name}{rest}"))
    }

    /// Build the surface; `catalog` is what a generated surface binds to.
    /// Errors are one line, ready for stderr and the in-window banner:
    /// `path:line:col: message` for markup.
    ///
    /// # Errors
    /// Unreadable file, markup parse error, or a broken bundle.
    pub fn load(&self, catalog: &Catalog) -> Result<Surface, String> {
        match self {
            Self::Generated { theme } => Ok(surface_for_catalog(
                catalog,
                &Options {
                    theme: (**theme).clone(),
                    ..Options::default()
                },
            )),
            Self::Markup(path) | Self::Bundle(path) => frame::document::io::load_surface(path)
                .map(Surface::from_scene)
                .map_err(|e| e.to_string()),
        }
    }
}

/// An empty dark surface to show (with the error banner) when the file is
/// broken at startup, so the window still opens and waits for a fix.
#[must_use]
pub fn placeholder() -> Surface {
    const SRC: &str = r##"<Document><Page><Frame w=960 h=360 fill="#0e0f12" /></Page></Document>"##;
    Surface::from_scene(
        frame::document::text::parse(SRC, BTreeMap::new()).expect("placeholder markup is valid"),
    )
}

/// How often to look at the file.
pub const POLL: Duration = Duration::from_millis(250);

/// A surface's identity for change detection: the mtimes and lengths of
/// its file and (for markup) its assets sidecar, so a same-second rewrite
/// on a coarse-mtime filesystem still registers, and so does a texture
/// swap. `None` while the file is missing.
pub type Stamp = Option<u64>;

#[must_use]
pub fn stamp(path: &Path) -> Stamp {
    frame::document::io::fingerprint(path)
}

/// Polls a stamp and says when to reload.
#[derive(Debug, Clone)]
pub struct Watch {
    last: Stamp,
    next: Instant,
}

impl Watch {
    #[must_use]
    pub fn new(initial: Stamp, now: Instant) -> Self {
        Self {
            last: initial,
            next: now + POLL,
        }
    }

    /// When the next check is due.
    #[must_use]
    pub fn deadline(&self) -> Instant {
        self.next
    }

    /// Whether a check is due at `now`.
    #[must_use]
    pub fn due(&self, now: Instant) -> bool {
        now >= self.next
    }

    /// Record the stamp seen at `now`; true when the file changed and
    /// exists (a vanished file — mid-save rename — waits for it to return).
    pub fn observe(&mut self, current: Stamp, now: Instant) -> bool {
        self.next = now + POLL;
        if current == self.last {
            return false;
        }
        self.last = current;
        current.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64, len: u64) -> Stamp {
        Some(secs * 1000 + len)
    }

    #[test]
    fn reloads_only_on_change() {
        let t0 = Instant::now();
        let mut w = Watch::new(at(1, 10), t0);
        assert!(!w.due(t0));
        assert!(w.due(t0 + POLL));
        assert!(!w.observe(at(1, 10), t0 + POLL));
        assert!(w.observe(at(2, 10), t0 + POLL * 2), "new mtime");
        assert!(!w.observe(at(2, 10), t0 + POLL * 3), "reported once");
        assert!(
            w.observe(at(2, 11), t0 + POLL * 4),
            "same mtime, new length"
        );
    }

    #[test]
    fn vanished_file_waits_for_it_to_return() {
        let t0 = Instant::now();
        let mut w = Watch::new(at(1, 10), t0);
        assert!(!w.observe(None, t0));
        assert!(!w.observe(None, t0));
        assert!(w.observe(at(1, 10), t0), "back (even unchanged) → reload");
    }

    #[test]
    fn observe_schedules_the_next_check() {
        let t0 = Instant::now();
        let mut w = Watch::new(None, t0);
        w.observe(None, t0 + POLL * 3);
        assert_eq!(w.deadline(), t0 + POLL * 4);
    }

    #[test]
    fn stamps_a_real_file() {
        let dir = std::env::temp_dir().join(format!("frame-desktop-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("s.frame");
        assert_eq!(stamp(&f), None);
        std::fs::write(&f, "abc").unwrap();
        let first = stamp(&f);
        assert!(first.is_some());
        std::fs::write(&f, "abcd").unwrap();
        assert_ne!(stamp(&f), first, "an edit changes it");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn markup_loads_with_its_textures_and_an_asset_change_is_seen() {
        let dir = std::env::temp_dir().join(format!("frame-desktop-assets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("tex.frame");
        let mut scene = placeholder().scene;
        scene.add_asset("font/ttf", b"font one".to_vec());
        frame::document::io::save_surface(&f, &scene).unwrap();
        let loaded = Source::from_path(f.clone())
            .load(&frame::surface::memory::demo_catalog())
            .unwrap();
        assert_eq!(loaded.scene.assets, scene.assets);

        let t0 = Instant::now();
        let mut w = Watch::new(stamp(&f), t0);
        assert!(!w.observe(stamp(&f), t0 + POLL));
        // Only the sidecar changes (markup doesn't name fonts): still a reload.
        scene.assets.clear();
        scene.add_asset("font/ttf", b"font two, longer".to_vec());
        frame::document::io::save_surface(&f, &scene).unwrap();
        assert!(w.observe(stamp(&f), t0 + POLL * 2));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn placeholder_has_a_page() {
        assert_eq!(placeholder().scene.pages().len(), 1);
    }

    #[test]
    fn picks_loader_by_content() {
        let dir =
            std::env::temp_dir().join(format!("frame-desktop-src-kind-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bundle = dir.join("b.frame");
        frame::document::io::save_surface_as(
            &bundle,
            &placeholder().scene,
            frame::document::io::Format::Bundle,
        )
        .unwrap();
        assert!(matches!(Source::from_path(bundle), Source::Bundle(_)));
        let text = dir.join("t.frame");
        std::fs::write(&text, "<Document/>").unwrap();
        assert!(matches!(Source::from_path(text), Source::Markup(_)));
        assert!(
            matches!(Source::from_path("a/new.frame".into()), Source::Markup(_)),
            "not written yet: markup"
        );
        assert!(matches!(
            Source::from_path("a/b.fm".into()),
            Source::Markup(_)
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn markup_errors_carry_path_line_and_col() {
        let dir = std::env::temp_dir().join(format!("frame-desktop-src-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("bad.frame");
        std::fs::write(&f, "<Document>\n  <Page>\n</Document>").unwrap();
        let err = Source::Markup(f.clone())
            .load(&frame::surface::memory::demo_catalog())
            .unwrap_err();
        assert!(err.starts_with(&format!("{}:", f.display())), "{err}");
        assert!(
            err.split(':')
                .nth(1)
                .is_some_and(|l| l.parse::<usize>().is_ok()),
            "{err}"
        );
        assert!(
            Source::Markup(f)
                .short_error(&err)
                .starts_with("bad.frame:"),
            "{err}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

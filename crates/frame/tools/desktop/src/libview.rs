//! The player's library mode (`--library`): browse a grid of previews,
//! preview one live against the engine, install or fork it — and follow
//! the surface on stage, swapping each new version in place.
//!
//! Host-agnostic like [`Player`]: the window and the headless loop both
//! drive it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use frame::document::proto::{NodeId, Scene};
use frame::library::link::{Signature, Surface as Stored};
use frame::surface::proto::Catalog;
use frame::surface::runtime::{Bounds, Surface};

use crate::browser::{self, Browser, Nav};
use crate::library::{self, Change, Live};
use crate::player::Player;

/// How long a toast stays up, ms.
const TOAST_MS: f64 = 6000.0;
/// …and an informational one.
const NOTE_MS: f64 = 2500.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The grid of previews.
    Browse,
    /// One surface on stage, followed live.
    Preview,
}

/// A small message over the bottom of the window.
#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub text: String,
    pub error: bool,
    /// Shown until this time (ms), from the first frame it's drawn.
    until: Option<f64>,
}

/// What a tick did (for the headless loop's screenshots and tests).
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// A new version went on stage.
    Applied { id: String, version: u64 },
    /// An update was refused; the stage kept the last good version.
    Rejected(String),
    /// The index or the link status changed (redraw the grid / banner).
    Changed,
}

pub struct LibraryView {
    live: Live,
    browser: Browser,
    mode: Mode,
    toast: Option<Toast>,
    install_dir: PathBuf,
    author: String,
}

impl LibraryView {
    /// Connect and list. `surface` starts in preview mode on it (unless
    /// `browse`, which starts on the grid with it selected).
    ///
    /// # Errors
    /// When the library can't be reached.
    pub fn start(
        url: &str,
        surface: Option<String>,
        browse: bool,
        install_dir: PathBuf,
    ) -> Result<Self, String> {
        let preview = surface.is_some() && !browse;
        let live = Live::connect(url, if preview { surface.clone() } else { None })?;
        let mut browser = Browser::new(url, None);
        browser.set_items(live.index());
        if let Some(id) = &surface {
            browser.select(id);
        }
        eprintln!("frame: library {url}: {} surfaces", live.index().len());
        let author = std::env::var("USER").unwrap_or_else(|_| "frame".into());
        Ok(Self {
            live,
            browser,
            mode: if preview { Mode::Preview } else { Mode::Browse },
            toast: None,
            install_dir,
            author,
        })
    }

    /// Only list surfaces that fit `engine` (named `name`).
    pub fn set_fits(&mut self, name: &str, engine: Signature) {
        self.browser = Browser::new(self.live.url(), Some((name.to_owned(), engine)));
        self.browser.set_items(self.live.index());
        if let Some(id) = self.live.following().map(str::to_owned) {
            self.browser.select(&id);
        }
    }

    /// The engine catalogs the library knows (to mock them).
    #[must_use]
    pub fn catalogs(&mut self) -> Vec<Catalog> {
        self.live.catalogs()
    }

    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    #[must_use]
    pub fn browsing(&self) -> bool {
        self.mode == Mode::Browse
    }

    /// The surface to start on in preview mode (its current version,
    /// fetched now), or `None` when browsing.
    ///
    /// # Errors
    /// Unreachable, no such surface, or it doesn't decode.
    pub fn initial(&mut self) -> Result<Option<Surface>, String> {
        let Some(id) = self.live.following().map(str::to_owned) else {
            return Ok(None);
        };
        let stored = self.live.get(&id)?;
        let scene = stored.content.decode().map_err(|e| format!("{id}:{e}"))?;
        self.live.shown(&stored);
        Ok(Some(Surface::from_scene(scene)))
    }

    /// Drain the library: new versions of the surface on stage go through
    /// [`library::accept`] and swap in place; the grid follows the index.
    pub fn tick(&mut self, player: &mut Player) -> Vec<Outcome> {
        let mut out = Vec::new();
        for change in self.live.poll() {
            match change {
                Change::Index => {
                    self.browser.set_items(self.live.index());
                    out.push(Outcome::Changed);
                }
                Change::Connected | Change::Lost(_) => out.push(Outcome::Changed),
                Change::Surface(update) => {
                    if self.mode != Mode::Preview {
                        continue;
                    }
                    let (id, version) = (update.id.clone(), update.version);
                    match library::accept(&update, player.scene(), player.catalog()).and_then(
                        |(stored, surface)| {
                            player.swap(surface)?;
                            Ok(stored)
                        },
                    ) {
                        Ok(stored) => {
                            eprintln!("frame: {id} v{version} live");
                            self.live.shown(&stored);
                            self.note(format!("{} — v{version}", stored.meta.name), false);
                            out.push(Outcome::Applied { id, version });
                        }
                        Err(e) => {
                            eprintln!("frame: {e}");
                            self.note(e.clone(), true);
                            out.push(Outcome::Rejected(e));
                        }
                    }
                }
            }
        }
        out
    }

    fn note(&mut self, text: String, error: bool) {
        self.toast = Some(Toast {
            text,
            error,
            until: None,
        });
    }

    /// Preview the selected surface live.
    pub fn open_selected(&mut self, player: &mut Player) {
        let Some(id) = self.browser.selected().map(|m| m.id.clone()) else {
            return;
        };
        self.open(&id, player);
    }

    fn open(&mut self, id: &str, player: &mut Player) {
        let result = self.live.get(id).and_then(|stored| {
            let scene = stored.content.decode().map_err(|e| format!("{id}: {e}"))?;
            player.swap(Surface::from_scene(scene))?;
            Ok(stored)
        });
        match result {
            Ok(stored) => {
                self.live.follow(Some(id.to_owned()));
                self.live.shown(&stored);
                self.mode = Mode::Preview;
                self.browser.select(id);
                eprintln!(
                    "frame: previewing {id} v{} (B for the library)",
                    stored.meta.version
                );
            }
            Err(e) => self.note(format!("can't open {id}: {e}"), true),
        }
    }

    /// Back to the grid.
    pub fn back(&mut self) {
        self.mode = Mode::Browse;
        self.live.follow(None);
    }

    pub fn nav(&mut self, nav: Nav) {
        if self.mode == Mode::Browse {
            self.browser.nav(nav);
        }
    }

    /// A click on the grid (scene px): select and preview that tile.
    pub fn click(&mut self, at: (f64, f64), player: &mut Player) -> bool {
        if self.mode != Mode::Browse {
            return false;
        }
        let Some(i) = self.browser.hit(at.0, at.1) else {
            return false;
        };
        self.browser.set_selected(i);
        self.open_selected(player);
        true
    }

    /// The surface the keys act on: on stage, or selected in the grid.
    fn current(&mut self) -> Result<Stored, String> {
        let id = match self.mode {
            Mode::Preview => self.live.following().map(str::to_owned),
            Mode::Browse => self.browser.selected().map(|m| m.id.clone()),
        }
        .ok_or("nothing selected")?;
        self.live.get(&id)
    }

    /// Copy the current surface into the install folder.
    pub fn install(&mut self) {
        match self
            .current()
            .and_then(|s| library::install(&s, &self.install_dir))
        {
            Ok(path) => {
                eprintln!("frame: installed {}", path.display());
                self.note(format!("installed to {}", path.display()), false);
            }
            Err(e) => self.note(format!("install failed: {e}"), true),
        }
    }

    /// Publish a fork of the current surface and preview it (edit it with
    /// `frame-mcp --library URL --surface <fork id>`, or on disk).
    pub fn fork(&mut self, player: &mut Player) {
        let author = self.author.clone();
        match self.current().and_then(|s| self.live.fork(&s, &author)) {
            Ok(p) => {
                eprintln!(
                    "frame: forked to {} — edit it: frame-mcp --library {} --surface {}",
                    p.id,
                    self.live.url(),
                    p.id
                );
                self.open(&p.id, player);
                self.note(format!("forked to {}", p.id), false);
            }
            Err(e) => self.note(format!("fork failed: {e}"), true),
        }
    }

    /// The grid, while browsing.
    pub fn document(&mut self) -> Option<&(Scene, NodeId)> {
        (self.mode == Mode::Browse).then(|| self.browser.scene())
    }

    #[must_use]
    pub fn content(&self) -> Bounds {
        self.browser.content()
    }

    #[must_use]
    pub fn background(&self) -> peniko::Color {
        browser::BACKGROUND
    }

    /// "library disconnected …" while it is.
    #[must_use]
    pub fn banner(&self) -> Option<String> {
        self.live
            .lost()
            .map(|why| format!("library disconnected — reconnecting… ({why})"))
    }

    #[must_use]
    pub fn title(&self) -> String {
        match (self.mode, self.live.following()) {
            (Mode::Preview, Some(id)) => format!(
                "frame — {id} — live from {} — B for the library",
                self.live.url()
            ),
            _ => format!("frame — library {}", self.live.url()),
        }
    }

    /// The toast at `t` ms, as a scene `width` logical px wide (with its
    /// size), or `None` once it expired.
    pub fn toast(&mut self, t: f64, width: f64) -> Option<(Scene, NodeId, f64, f64)> {
        let toast = self.toast.as_mut()?;
        let until = *toast
            .until
            .get_or_insert(t + if toast.error { TOAST_MS } else { NOTE_MS });
        if t > until {
            self.toast = None;
            return None;
        }
        toast_scene(&toast.text, toast.error, width)
    }

    #[cfg(test)]
    pub fn toast_text(&self) -> Option<&Toast> {
        self.toast.as_ref()
    }
}

#[cfg(test)]
mod tests;

/// A rounded message box, up to 760 logical px wide.
#[must_use]
pub fn toast_scene(text: &str, error: bool, width: f64) -> Option<(Scene, NodeId, f64, f64)> {
    let w = (width - 32.0).clamp(80.0, 760.0);
    let h = 34.0;
    let fill = if error { "#b3261ef0" } else { "#2b2f3af0" };
    let src = format!(
        r##"<Document><Page><Frame w={w} h={h} radius=8 clip fill="{fill}" stroke="#ffffff22"><Text x=12 y=9 w={tw} h=16 size=12 fill="#ffffff" text={t} /></Frame></Page></Document>"##,
        tw = w - 24.0,
        t = serde_json::to_string(text).ok()?,
    );
    let scene = frame::document::text::parse(&src, BTreeMap::new()).ok()?;
    let page = *scene.pages().first()?;
    Some((scene, page, w, h))
}

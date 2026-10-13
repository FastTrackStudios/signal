//! `--library`: surfaces from a library server, followed live.
//!
//! [`Live`] owns the link: a [`Follow`] thread pushing the followed
//! surface's updates and the index changes, and a [`Blocking`] client for
//! the calls a key press makes (get, publish a fork). [`accept`] decides
//! whether an update may replace what's on stage.

use std::path::{Path, PathBuf};
use std::time::Duration;

use design_lint::{Config, Severity};
use document_proto::Scene;
use library_link::{
    Blocking, Event, Follow, ListQuery, PublishMeta, Published, Surface as Stored, SurfaceMeta,
    SurfaceUpdate,
};
use surface_proto::Catalog;
use surface_runtime::Surface;

/// How long startup waits for the library.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// What happened on the link since the last poll.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// The followed surface has a new version.
    Surface(Box<SurfaceUpdate>),
    /// The index changed (the browser should refresh).
    Index,
    /// The link dropped; reconnecting.
    Lost(String),
    /// The link is back.
    Connected,
}

pub struct Live {
    follow: Follow,
    client: Blocking,
    /// The surface being followed.
    following: Option<String>,
    /// Every surface's metadata, kept current from the index stream.
    index: Vec<SurfaceMeta>,
    /// What's on stage: (id, version, content hash), to skip echoes.
    shown: Option<(String, u64, u64)>,
    /// Why the library is unreachable, while it is.
    lost: Option<String>,
}

impl Live {
    /// Connect to `url` (blocking until it answers) and list it.
    ///
    /// # Errors
    /// When the library can't be reached.
    pub fn connect(url: &str, follow: Option<String>) -> Result<Self, String> {
        let mut client = Blocking::new(url)?;
        let deadline = std::time::Instant::now() + CONNECT_TIMEOUT;
        let index = loop {
            match client.list(&ListQuery::default()) {
                Ok(index) => break index,
                Err(e) if std::time::Instant::now() >= deadline => {
                    return Err(format!("can't reach the library at {url}: {e}"));
                }
                Err(_) => std::thread::sleep(Duration::from_millis(200)),
            }
        };
        let mut index = index;
        fill_previews(&mut client, &mut index);
        let follow_thread = Follow::start(url, follow.clone())?;
        Ok(Self {
            follow: follow_thread,
            client,
            following: follow,
            index,
            shown: None,
            lost: None,
        })
    }

    #[must_use]
    pub fn url(&self) -> &str {
        self.client.url()
    }

    /// The engine catalogs the library knows.
    #[must_use]
    pub fn catalogs(&mut self) -> Vec<Catalog> {
        self.client.catalogs().unwrap_or_default()
    }

    #[must_use]
    pub fn index(&self) -> &[SurfaceMeta] {
        &self.index
    }

    #[must_use]
    pub fn following(&self) -> Option<&str> {
        self.following.as_deref()
    }

    #[must_use]
    pub fn lost(&self) -> Option<&str> {
        self.lost.as_deref()
    }

    /// Follow `id` (or nothing) from now on.
    pub fn follow(&mut self, id: Option<String>) {
        if self.following != id {
            self.following.clone_from(&id);
            self.follow.follow(id);
        }
    }

    /// Fetch a surface now (a key press; the follow stream catches up).
    ///
    /// # Errors
    /// Unreachable, or no such surface.
    pub fn get(&mut self, id: &str) -> Result<Stored, String> {
        self.client.get(id, None)
    }

    /// Publish a copy of `surface` as a fork.
    ///
    /// # Errors
    /// Unreachable, or the library refused it.
    pub fn fork(&mut self, surface: &Stored, author: &str) -> Result<Published, String> {
        let m = &surface.meta;
        let meta = PublishMeta {
            id: String::new(),
            name: format!("{} (fork)", m.name),
            author: author.to_owned(),
            description: m.description.clone(),
            tags: m.tags.clone(),
            license: m.license.clone(),
            forked_from: Some(format!("{}@{}", m.id, m.version)),
            preview_png: m.preview_png.clone(),
        };
        self.client.publish(&surface.content, &meta)
    }

    /// Record what's on stage (so its own echo isn't applied twice).
    pub fn shown(&mut self, stored: &Stored) {
        self.shown = Some((
            stored.meta.id.clone(),
            stored.meta.version,
            hash(&stored.content.bytes),
        ));
    }

    /// Drain the link. Updates for the followed surface that differ from
    /// what's shown come back as [`Change::Surface`].
    pub fn poll(&mut self) -> Vec<Change> {
        let mut out = Vec::new();
        for event in self.follow.drain() {
            match event {
                Event::Connected => {
                    if self.lost.take().is_some() {
                        // Missed index changes while away: re-list.
                        if let Ok(mut index) = self.client.list(&ListQuery::default()) {
                            fill_previews(&mut self.client, &mut index);
                            self.index = index;
                        }
                        out.push(Change::Index);
                    }
                    out.push(Change::Connected);
                }
                Event::Lost(why) => {
                    self.lost = Some(why.clone());
                    out.push(Change::Lost(why));
                }
                Event::Index(c) => {
                    self.index.retain(|m| m.id != c.id);
                    if let Some(meta) = c.meta {
                        let at = self.index.partition_point(|m| m.id < meta.id);
                        self.index.insert(at, meta);
                    }
                    out.push(Change::Index);
                }
                Event::Update(u) => {
                    if Some(u.id.as_str()) != self.following.as_deref() {
                        continue; // from the surface followed before a switch
                    }
                    let same = match (&self.shown, &u.surface) {
                        (Some((id, _, h)), Some(s)) => *id == u.id && *h == hash(&s.content.bytes),
                        _ => false,
                    };
                    if !same {
                        out.push(Change::Surface(Box::new(u)));
                    }
                }
            }
        }
        out
    }
}

fn hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// Whether an update may go on stage: it must decode, and must not bring
/// lint errors the current design doesn't already have (matched by rule
/// and node name, so markup without ids compares too) (a half-finished
/// edit mustn't break a live stage). Errors are one line for a toast.
///
/// # Errors
/// Why the update was rejected.
pub fn accept(
    update: &SurfaceUpdate,
    current: &Scene,
    catalog: &Catalog,
) -> Result<(Stored, Surface), String> {
    let who = format!("{} v{}", update.id, update.version);
    let stored = update
        .surface
        .clone()
        .ok_or_else(|| format!("{} was removed from the library", update.id))?;
    let scene = stored
        .content
        .decode()
        .map_err(|e| format!("{who} rejected: {e}"))?;
    let errors = |s: &Scene| -> Vec<(String, String)> {
        design_lint::lint(s, catalog, &Config::default())
            .into_iter()
            .filter(|i| i.severity == Severity::Error)
            .map(|i| (i.rule, i.node_name))
            .collect()
    };
    let before = errors(current);
    let after = design_lint::lint(&scene, catalog, &Config::default());
    if let Some(new) = after.iter().find(|i| {
        i.severity == Severity::Error && !before.contains(&(i.rule.clone(), i.node_name.clone()))
    }) {
        return Err(format!(
            "{who} rejected: {}[{}] \"{}\": {}",
            "lint", new.rule, new.node_name, new.message
        ));
    }
    Ok((stored, Surface::from_scene(scene)))
}

/// Copy a surface into `dir` (`<dir>/<id>.<ext>`, plus `<id>.frame.assets/`
/// for markup that carries textures or fonts), returning the path.
///
/// # Errors
/// When it can't be written.
pub fn install(stored: &Stored, dir: &Path) -> Result<PathBuf, String> {
    let rel: PathBuf = stored
        .meta
        .id
        .split('/')
        .filter(|p| !p.is_empty() && *p != "..")
        .collect();
    let path = dir
        .join(rel)
        .with_extension(stored.content.format.extension());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    match stored.content.format {
        library_link::Format::Markup => {
            let text = std::str::from_utf8(&stored.content.bytes)
                .map_err(|e| format!("{}: not UTF-8: {e}", path.display()))?;
            document_io::write_markup(&path, text, &stored.content.assets)
                .map_err(|e| e.to_string())?;
        }
        library_link::Format::Frame => {
            std::fs::write(&path, &stored.content.bytes)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            document_io::prune_assets(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(path)
}

/// Where installs go by default: `$XDG_DATA_HOME/frame/surfaces`, else
/// `~/.local/share/frame/surfaces`, else `./surfaces`.
#[must_use]
pub fn default_install_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .map_or_else(|| PathBuf::from("surfaces"), |d| d.join("frame/surfaces"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use library_link::{Content, SurfaceMeta};

    const GOOD: &str = r##"<Document><Page><Frame name="Panel" w=200 h=100 fill="#101010"><Frame name="Gain" w=60 h=60 control={{"binding":{"curve":"Linear","max":1.0,"min":0.0,"source":{"Param":{"addr":"amp/gain"}}},"kind":{"Knob":{"sweep_degrees":270.0}},"slots":[]}} /></Frame></Page></Document>"##;

    fn update(text: &str) -> SurfaceUpdate {
        SurfaceUpdate {
            id: "t".into(),
            version: 2,
            surface: Some(Stored {
                meta: SurfaceMeta {
                    id: "t".into(),
                    version: 2,
                    ..SurfaceMeta::default()
                },
                content: Content::markup_text(text),
            }),
        }
    }

    fn current() -> Scene {
        document_text::parse(GOOD, Default::default()).unwrap()
    }

    #[test]
    fn a_good_update_is_accepted() {
        let (_, s) = accept(
            &update(&GOOD.replace("w=200", "w=240")),
            &current(),
            &surface_memory::demo_catalog(),
        )
        .unwrap();
        assert_eq!(s.controls.len(), 1);
    }

    #[test]
    fn a_malformed_update_is_rejected_with_its_position() {
        let err = accept(
            &update("<Document>\n  <Page>\n"),
            &current(),
            &surface_memory::demo_catalog(),
        )
        .unwrap_err();
        assert!(err.starts_with("t v2 rejected: "), "{err}");
        assert!(
            err.trim_start_matches("t v2 rejected: ")
                .split(':')
                .next()
                .unwrap()
                .parse::<usize>()
                .is_ok(),
            "line:col: {err}"
        );
    }

    #[test]
    fn an_update_that_breaks_a_binding_is_rejected() {
        let broken = GOOD.replace("amp/gain", "amp/nope");
        let err = accept(
            &update(&broken),
            &current(),
            &surface_memory::demo_catalog(),
        )
        .unwrap_err();
        assert!(err.contains("lint[binding]"), "{err}");
        // …but errors the stage already has don't block other edits.
        let current_broken = document_text::parse(&broken, Default::default()).unwrap();
        assert!(
            accept(
                &update(&broken.replace("w=200", "w=220")),
                &current_broken,
                &surface_memory::demo_catalog()
            )
            .is_ok()
        );
    }

    #[test]
    fn a_removal_is_reported() {
        let gone = SurfaceUpdate {
            id: "t".into(),
            version: 0,
            surface: None,
        };
        assert!(
            accept(&gone, &current(), &surface_memory::demo_catalog())
                .unwrap_err()
                .contains("removed")
        );
    }

    #[test]
    fn install_writes_under_the_id() {
        let dir = std::env::temp_dir().join(format!("frame-install-{}", std::process::id()));
        let stored = update(GOOD).surface.unwrap();
        let stored = Stored {
            meta: SurfaceMeta {
                id: "comp/../x/y".into(),
                ..stored.meta
            },
            ..stored
        };
        let path = install(&stored, &dir).unwrap();
        assert_eq!(path, dir.join("comp/x/y.frame"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), GOOD);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn install_brings_the_textures_along() {
        let dir = std::env::temp_dir().join(format!("frame-install-assets-{}", std::process::id()));
        let mut scene = current();
        scene.add_asset("image/png", b"texture".to_vec());
        let stored = Stored {
            meta: SurfaceMeta {
                id: "amp/tex".into(),
                version: 1,
                ..SurfaceMeta::default()
            },
            content: Content::markup(&scene),
        };
        let path = install(&stored, &dir).unwrap();
        assert_eq!(document_io::load_surface(&path).unwrap(), scene);
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Fetch each listed surface's preview one call at a time: the list comes
/// without them, since a big library's previews outgrow one message.
fn fill_previews(client: &mut Blocking, index: &mut [SurfaceMeta]) {
    for meta in index.iter_mut().filter(|m| m.preview_png.is_empty()) {
        if let Ok(png) = client.preview(&meta.id) {
            meta.preview_png = png;
        }
    }
}

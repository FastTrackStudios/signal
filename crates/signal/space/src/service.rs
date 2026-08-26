//! The engine-side `SampleSpace` service: discovers built `.space` stores
//! under the configured roots, serves map/similarity queries, auditions
//! through the system output, and runs (re)builds off-thread with progress
//! events.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use architect::dispatch::CurrentThreadDispatcher;
use architect::{HasDispatcher, Layer, PubSub, Services, layers};
use signal_space_proto::space::SampleSpace;
use signal_space_proto::{MapItem, SimilarHit, SpaceEvent, SpaceFilter, SpaceInfo};

use crate::{Space, build, knn};

/// Re-exported for consumers that read the same roots (the ekit rig).
pub use crate::ROOTS_ENV;

struct Loaded {
    dir: PathBuf,
    space: Space,
    features: Vec<f32>,
}

#[derive(Clone, HasDispatcher)]
#[dispatch(CurrentThreadDispatcher)]
pub struct SpaceBackend {
    inner: Arc<Inner>,
}

struct Inner {
    /// name → loaded space (lazy).
    cache: Mutex<HashMap<String, Arc<Loaded>>>,
    events: PubSub<SpaceEvent>,
    /// PID of the current audition player, killed on the next audition.
    audition: Mutex<Option<std::process::Child>>,
}

impl Default for SpaceBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SpaceBackend {
    pub fn new() -> Self {
        tracing::info!(roots = ?crate::space_roots(), "sample space: roots");
        Self {
            inner: Arc::new(Inner {
                cache: Mutex::new(HashMap::new()),
                events: architect::rig::events_hub(),
                audition: Mutex::new(None),
            }),
        }
    }

    pub fn router(&self) -> architect::LayerRouter {
        self.clone().into_router()
    }

    /// All `.space` dirs under the roots.
    fn discover(&self) -> Vec<PathBuf> {
        crate::discover_spaces()
    }

    fn load(&self, name: &str) -> Option<Arc<Loaded>> {
        if let Some(l) = self.inner.cache.lock().unwrap().get(name) {
            return Some(l.clone());
        }
        let dir = self
            .discover()
            .into_iter()
            .find(|d| d.file_stem().and_then(|s| s.to_str()) == Some(name))?;
        let (space, features) = Space::load(&dir).ok()?;
        let loaded = Arc::new(Loaded {
            dir,
            space,
            features,
        });
        self.inner
            .cache
            .lock()
            .unwrap()
            .insert(name.to_string(), loaded.clone());
        Some(loaded)
    }

    /// Resolve an item to a playable file: sample items are relative paths;
    /// piece items are directory keys — pick their middle wav.
    fn resolve_audio(space: &Space, idx: usize) -> Option<PathBuf> {
        let item = space.items.get(idx)?;
        let root = Path::new(&space.root);
        let direct = root.join(&item.path);
        if direct.is_file() {
            return Some(direct);
        }
        let mut wavs: Vec<PathBuf> = walkdir::WalkDir::new(root.join(&item.path))
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.into_path())
            .filter(|p| {
                p.extension()
                    .and_then(|x| x.to_str())
                    .is_some_and(|x| x.eq_ignore_ascii_case("wav"))
            })
            .collect();
        wavs.sort();
        let mid = wavs.len() / 2;
        wavs.into_iter().nth(mid)
    }
}

fn matches(item: &crate::SpaceItem, f: &SpaceFilter) -> bool {
    (f.classes.is_empty() || f.classes.iter().any(|c| c == &item.class))
        && (f.text.is_empty() || item.path.to_lowercase().contains(&f.text.to_lowercase()))
        && (!f.favorites_only || item.favorite)
        && (f.max_duration_s <= 0.0 || item.duration_s <= f.max_duration_s)
}

impl SampleSpace for SpaceBackend {
    fn spaces(&self) -> Vec<SpaceInfo> {
        self.discover()
            .into_iter()
            .filter_map(|dir| {
                let name = dir.file_stem()?.to_str()?.to_string();
                let loaded = self.load(&name)?;
                Some(SpaceInfo {
                    name,
                    root: loaded.space.root.clone(),
                    item_count: loaded.space.items.len() as u32,
                })
            })
            .collect()
    }

    fn map(&self, space: String, filter: SpaceFilter) -> Vec<MapItem> {
        let Some(l) = self.load(&space) else {
            return Vec::new();
        };
        l.space
            .items
            .iter()
            .enumerate()
            .filter(|(_, it)| matches(it, &filter))
            .map(|(idx, it)| MapItem {
                idx: idx as u32,
                path: it.path.clone(),
                class: it.class.clone(),
                x: it.x,
                y: it.y,
                duration_s: it.duration_s,
                centroid_hz: it.centroid_hz,
                percussiveness: it.percussiveness,
                favorite: it.favorite,
            })
            .collect()
    }

    fn similar(&self, space: String, idx: u32, filter: SpaceFilter) -> Vec<SimilarHit> {
        let Some(l) = self.load(&space) else {
            return Vec::new();
        };
        let items = &l.space.items;
        if idx as usize >= items.len() {
            return Vec::new();
        }
        knn::similar(&l.features, l.space.dim, idx as usize, 16, |i| {
            matches(&items[i], &filter)
        })
        .into_iter()
        .map(|(i, score)| SimilarHit {
            idx: i as u32,
            path: items[i].path.clone(),
            class: items[i].class.clone(),
            score,
        })
        .collect()
    }

    fn audition(&self, space: String, idx: u32) {
        let Some(l) = self.load(&space) else { return };
        let Some(path) = Self::resolve_audio(&l.space, idx as usize) else {
            return;
        };
        // Placeholder preview path until a proper engine preview lane lands:
        // a PipeWire client per audition (default sink), previous one killed.
        let mut slot = self.inner.audition.lock().unwrap();
        if let Some(mut old) = slot.take() {
            let _ = old.kill();
            let _ = old.wait();
        }
        match std::process::Command::new("pw-play")
            .arg("--media-name")
            .arg("fts-space-audition")
            .arg(&path)
            .spawn()
        {
            Ok(child) => *slot = Some(child),
            Err(e) => tracing::warn!("space audition: pw-play failed: {e}"),
        }
    }

    fn set_favorite(&self, space: String, idx: u32, favorite: bool) {
        let Some(l) = self.load(&space) else { return };
        let mut s = l.space.clone();
        if let Some(it) = s.items.get_mut(idx as usize) {
            it.favorite = favorite;
        }
        if s.save(&l.dir, &l.features).is_ok() {
            self.inner.cache.lock().unwrap().insert(
                space,
                Arc::new(Loaded {
                    dir: l.dir.clone(),
                    space: s,
                    features: l.features.clone(),
                }),
            );
            self.inner.events.publish(SpaceEvent::Changed);
        }
    }

    fn build(&self, name: String, root: String, pieces: bool) {
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("space-build".into())
            .spawn(move || {
                let root = PathBuf::from(&root);
                let granularity = if pieces {
                    build::Granularity::Piece
                } else {
                    build::Granularity::Sample
                };
                let dir = Space::space_dir(&root, &name);
                let previous = Space::load(&dir).ok();
                let events = b.inner.events.clone();
                let n2 = name.clone();
                let report = build::build(
                    &name,
                    &root,
                    granularity,
                    previous.as_ref().map(|(s, f)| (s, f.as_slice())),
                    &move |n, total| {
                        events.publish(SpaceEvent::Progress(n2.clone(), n as u32, total as u32));
                    },
                );
                match report.space.save(&dir, &report.features) {
                    Ok(()) => {
                        b.inner.cache.lock().unwrap().remove(&name);
                        tracing::info!(
                            name,
                            items = report.space.items.len(),
                            analyzed = report.analyzed,
                            "space build done"
                        );
                    }
                    Err(e) => tracing::error!("space build save failed: {e}"),
                }
                b.inner.events.publish(SpaceEvent::Changed);
            });
    }
}

impl signal_space_proto::space::SampleSpaceStreamSource for SpaceBackend {
    fn events_hub(&self) -> &PubSub<SpaceEvent> {
        &self.inner.events
    }
}

impl Services for SpaceBackend {
    fn layers() -> impl Layer<Self> {
        layers![
            signal_space_proto::space::Service,
            signal_space_proto::space::StreamService
        ]
    }
}

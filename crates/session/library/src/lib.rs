//! The song library, in Task — the same one keyflow's library reads.
//!
//! A **song** is a `resources_proto::SongDoc` (`song:<slug>`) with its
//! chart attached; a **setlist** is a `songlist` collection of `song:`
//! references in order; a song's **session** — `.RPP`, prepared
//! `.session`, `.lrc`, `Media/Proxies/*.ogg`, and the originals when
//! they were uploaded — is a File Root at `session/<slug>`. Nothing here
//! is Session-specific in Task: the schema is keyflow's and ADR 0004's,
//! and the File Root is found by its path.
//!
//! [`Library`] dials an org with `task-dial` (the dial the web app and
//! the CLI share), lists setlists, and pulls a song's session into a
//! local folder that the app opens like any other.

use std::path::{Path, PathBuf};

use collection_proto::{CollectionKind, CollectionServiceClient, Placement};
use files_client::FilesClient;
use files_proto::{
    MediaServiceClient, MediaServiceStreamClient, RootPath, RootsServiceClient, TreeServiceClient,
    UploadServiceClient,
};
use links_proto::{NodeKind, NodeRef};
use resources_proto::ResourcesServiceClient;

/// The collection kind a setlist is — keyflow's word for it.
pub const SETLIST_KIND: &str = "songlist";

/// A set for a service or a show ("JHM Sunday"), in the order it is
/// played — Task's own word for one.
pub const SET_KIND: &str = "setlist";

/// Every collection kind that is songs in an order, and can be played as
/// one: a song list (keyflow's `songlist` — "Worship Tracks", a shelf the
/// sets are picked from) and a set ([`SET_KIND`]).
pub const SETLIST_KINDS: [&str; 2] = [SETLIST_KIND, SET_KIND];

/// What a list is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ListKind {
    /// Songs gathered to pick from ("Worship Tracks").
    Songs,
    /// A set, in the order it is played ("JHM Sunday").
    Set,
}

impl ListKind {
    /// The collection kind Task files it as.
    #[must_use]
    pub const fn collection_kind(self) -> &'static str {
        match self {
            Self::Songs => SETLIST_KIND,
            Self::Set => SET_KIND,
        }
    }

    /// The kind a collection kind is, if it is a list of songs at all.
    #[must_use]
    pub fn of(kind: &str) -> Option<Self> {
        match kind {
            SETLIST_KIND => Some(Self::Songs),
            SET_KIND => Some(Self::Set),
            _ => None,
        }
    }
}

/// The File Root a song's session lives in.
#[must_use]
pub fn session_root_dir(song_slug: &str) -> String {
    format!("session/{song_slug}")
}

/// A song's library id from its title (`God, I'm` → `god-im`) — the rule
/// lives with the live session's code, which matches songs by it too.
pub use session_sync::slug::slugify;

/// The token the `task` CLI stored for `org`:
/// `$XDG_DATA_HOME/task/session-tokens/<org>.json` (default data home
/// `~/.local/share`), field `token`.
fn task_cli_token(org: &str) -> Option<String> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    let text =
        std::fs::read_to_string(data.join("task/session-tokens").join(format!("{org}.json")))
            .ok()?;
    // `"token":"…"` — a JWT, so no escapes to undo.
    let rest = text.split("\"token\"").nth(1)?;
    let start = rest.find('"')? + 1;
    let end = start + rest.get(start..)?.find('"')?;
    rest.get(start..end).map(str::to_owned)
}

/// One list of songs, in order: a set, or a song list ([`ListKind`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setlist {
    pub id: String,
    pub title: String,
    pub kind: ListKind,
    pub songs: Vec<Song>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Song {
    pub slug: String,
    pub title: String,
    pub writers: Vec<String>,
    pub key: String,
}

/// An org's library on a Task server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    /// `ws://host:port` — the server, without the per-org path.
    pub server: String,
    pub org: String,
    /// The signed-in person's session token, when the server wants one.
    pub token: Option<String>,
}

impl Library {
    /// From `FTS_TASK_SERVER` (default `ws://127.0.0.1:18080`, Task's
    /// local server), `FTS_TASK_ORG` (default `acme-audio`, its demo
    /// org) and `FTS_TASK_TOKEN` — or, without one, the session the
    /// `task` CLI keeps after `task auth login` (so signing in once there
    /// is signing in here).
    #[must_use]
    pub fn from_env() -> Self {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let org = var("FTS_TASK_ORG").unwrap_or_else(|| "acme-audio".into());
        let token = var("FTS_TASK_TOKEN").or_else(|| task_cli_token(&org));
        Self {
            server: var("FTS_TASK_SERVER").unwrap_or_else(|| "ws://127.0.0.1:18080".into()),
            org,
            token,
        }
    }

    /// The org's vox lane — where a member reaches every org service
    /// (the library, a set's live session).
    #[must_use]
    pub fn org_url(&self) -> String {
        format!("{}/org/{}/vox", self.server.trim_end_matches('/'), self.org)
    }

    /// The server over HTTP — `https://host` for `wss://host`.
    #[must_use]
    pub fn http_base(&self) -> String {
        self.server
            .trim_end_matches('/')
            .replacen("wss://", "https://", 1)
            .replacen("ws://", "http://", 1)
    }

    async fn client<C: vox_core::FromVoxLane + 'static>(&self) -> eyre::Result<C> {
        task_dial::establish_at::<C>(&self.org_url(), self.token.as_deref())
            .await
            .map_err(|e| eyre::eyre!("dialling {}: {e}", self.org_url()))
    }

    async fn files(&self) -> eyre::Result<FilesClient> {
        Ok(FilesClient::new(
            self.client::<RootsServiceClient>().await?,
            self.client::<TreeServiceClient>().await?,
            self.client::<UploadServiceClient>().await?,
            self.client::<MediaServiceClient>().await?,
            self.client::<MediaServiceStreamClient>().await?,
        ))
    }

    /// Every setlist in the org, of either kind ([`SETLIST_KINDS`]), with
    /// its songs in order.
    ///
    /// A song is looked up once however many sets it is in, and the
    /// lookups go at once: one after another, an org's dozen sets of a
    /// dozen songs was a hundred and forty round trips before the start
    /// screen had anything to show.
    ///
    /// # Errors
    /// When the server cannot be reached or refuses.
    pub async fn setlists(&self) -> eyre::Result<Vec<Setlist>> {
        use futures_util::StreamExt as _;
        /// Song lookups in flight at once.
        const AT_ONCE: usize = 16;
        let collections: CollectionServiceClient = self.client().await?;
        let resources: ResourcesServiceClient = self.client().await?;
        let mut lists = Vec::new();
        for kind in SETLIST_KINDS {
            lists.extend(
                collections
                    .list(self.org.clone(), Some(CollectionKind::new(kind)))
                    .await
                    .map_err(|e| eyre::eyre!("listing setlists: {e:?}"))?,
            );
        }
        let mut slugs: Vec<String> = lists
            .iter()
            .flat_map(|list| list.items.iter())
            .filter(|i| i.node.kind == NodeKind::Song)
            .map(|i| i.node.id.clone())
            .collect();
        slugs.sort();
        slugs.dedup();
        let found: std::collections::HashMap<String, Song> = futures_util::stream::iter(slugs)
            .map(|slug| {
                let resources = &resources;
                async move {
                    let doc = resources.song(slug.clone()).await.ok()?;
                    Some((
                        slug.clone(),
                        Song {
                            slug,
                            title: doc.title,
                            writers: doc.writers,
                            key: doc.key,
                        },
                    ))
                }
            })
            .buffer_unordered(AT_ONCE)
            .filter_map(std::future::ready)
            .collect()
            .await;
        Ok(lists
            .into_iter()
            .map(|mut list| {
                list.sort_items();
                let kind = ListKind::of(list.kind.as_str()).unwrap_or(ListKind::Songs);
                let songs = list
                    .items
                    .iter()
                    .filter(|i| i.node.kind == NodeKind::Song)
                    .map(|item| {
                        found.get(&item.node.id).cloned().unwrap_or_else(|| {
                            // A dangling reference still has a place in the order.
                            Song {
                                title: item.node.id.clone(),
                                slug: item.node.id.clone(),
                                writers: Vec::new(),
                                key: String::new(),
                            }
                        })
                    })
                    .collect();
                Setlist {
                    id: list.id,
                    title: list.title,
                    kind,
                    songs,
                }
            })
            .collect())
    }

    /// Every song in the org, by title, and whether each has a session to
    /// play (a `session/<slug>` File Root) — a song without one can be
    /// listed and set, but plays nothing yet.
    ///
    /// # Errors
    /// When the server cannot be reached or refuses.
    pub async fn songs(&self) -> eyre::Result<Vec<(Song, bool)>> {
        let resources: ResourcesServiceClient = self.client().await?;
        let listed = resources
            .list_songs()
            .await
            .map_err(|e| eyre::eyre!("listing songs: {e:?}"))?;
        let files = self.files().await?;
        let roots = files
            .roots
            .list()
            .await
            .map_err(|e| eyre::eyre!("listing roots: {e:?}"))?;
        let has_session = |slug: &str| {
            let dir = session_root_dir(slug);
            roots.iter().any(|r| {
                r.path
                    .as_deref()
                    .is_some_and(|p| p.trim_end_matches('/').ends_with(&dir))
            })
        };
        let mut songs: Vec<(Song, bool)> = listed
            .into_iter()
            .map(|s| {
                let playable = has_session(&s.slug);
                (
                    Song {
                        slug: s.slug,
                        title: s.title,
                        writers: s.writers,
                        key: s.key,
                    },
                    playable,
                )
            })
            .collect();
        songs.sort_by(|a, b| a.0.title.to_lowercase().cmp(&b.0.title.to_lowercase()));
        Ok(songs)
    }

    /// A new song called `title`, in `key` (empty when unset), by
    /// `writers` — no chart and no session yet: it can be put in lists at
    /// once, and plays once a session is made for it. The server makes its
    /// slug from the title.
    ///
    /// # Errors
    /// An empty title, or the server refused.
    pub async fn create_song(
        &self,
        title: &str,
        key: &str,
        writers: &[String],
    ) -> eyre::Result<Song> {
        let title = title.trim();
        if title.is_empty() {
            eyre::bail!("a song needs a title");
        }
        let resources: ResourcesServiceClient = self.client().await?;
        let made = resources
            .upsert_song(resources_proto::SongDoc {
                title: title.to_owned(),
                key: key.trim().to_owned(),
                writers: writers.to_vec(),
                ..resources_proto::SongDoc::default()
            })
            .await
            .map_err(|e| eyre::eyre!("making {title}: {e:?}"))?;
        Ok(Song {
            slug: made.slug,
            title: title.to_owned(),
            writers: writers.to_vec(),
            key: key.trim().to_owned(),
        })
    }

    /// Change song `song.slug`'s title, key and writers to `song`'s — its
    /// tags, chart and session as they were.
    ///
    /// # Errors
    /// No such song, an empty title, or the server refused.
    pub async fn update_song(&self, song: &Song) -> eyre::Result<()> {
        if song.title.trim().is_empty() {
            eyre::bail!("a song needs a title");
        }
        let resources: ResourcesServiceClient = self.client().await?;
        let mut doc = resources
            .song(song.slug.clone())
            .await
            .map_err(|e| eyre::eyre!("reading {}: {e:?}", song.slug))?;
        doc.title = song.title.trim().to_owned();
        doc.key = song.key.trim().to_owned();
        doc.writers.clone_from(&song.writers);
        resources
            .upsert_song(doc)
            .await
            .map_err(|e| eyre::eyre!("saving {}: {e:?}", song.title))?;
        Ok(())
    }

    /// Delete song `slug` from the library. Lists that name it keep a
    /// reference to nothing, which they skip.
    ///
    /// # Errors
    /// The server refused.
    pub async fn delete_song(&self, slug: &str) -> eyre::Result<()> {
        let resources: ResourcesServiceClient = self.client().await?;
        resources
            .delete_song(slug.to_owned())
            .await
            .map_err(|e| eyre::eyre!("deleting {slug}: {e:?}"))?;
        Ok(())
    }

    /// A copy of list `from` called `title`, of the same kind, with its
    /// songs in the same order — last week's set as the start of this
    /// week's.
    ///
    /// # Errors
    /// No such list, or the server refused.
    pub async fn duplicate_list(&self, from: &Setlist, title: &str) -> eyre::Result<Setlist> {
        let mut made = self.create_list(title, from.kind).await?;
        let order: Vec<String> = from.songs.iter().map(|s| s.slug.clone()).collect();
        self.set_songs(&made.id, &order).await?;
        made.songs.clone_from(&from.songs);
        Ok(made)
    }

    /// A new, empty list called `title`.
    ///
    /// # Errors
    /// An empty title, or the server refused.
    pub async fn create_list(&self, title: &str, kind: ListKind) -> eyre::Result<Setlist> {
        let collections: CollectionServiceClient = self.client().await?;
        let made = collections
            .create(
                self.org.clone(),
                title.to_owned(),
                CollectionKind::new(kind.collection_kind()),
            )
            .await
            .map_err(|e| eyre::eyre!("creating {title}: {e:?}"))?;
        Ok(Setlist {
            id: made.id,
            title: made.title,
            kind,
            songs: Vec::new(),
        })
    }

    /// Call list `id` `title`.
    ///
    /// # Errors
    /// No such list, an empty title, or the server refused.
    pub async fn rename_list(&self, id: &str, title: &str) -> eyre::Result<()> {
        let collections: CollectionServiceClient = self.client().await?;
        collections
            .rename(id.to_owned(), title.to_owned())
            .await
            .map_err(|e| eyre::eyre!("renaming: {e:?}"))?;
        Ok(())
    }

    /// Delete list `id` — the list only: its songs stay in the library
    /// and in every other list.
    ///
    /// # Errors
    /// No such list, or the server refused.
    pub async fn delete_list(&self, id: &str) -> eyre::Result<()> {
        let collections: CollectionServiceClient = self.client().await?;
        collections
            .delete(id.to_owned())
            .await
            .map_err(|e| eyre::eyre!("deleting: {e:?}"))?;
        Ok(())
    }

    /// Make list `id`'s songs `songs` (slugs, in order): what is missing
    /// added, what is gone removed, and what is out of place moved, one
    /// change at a time. How an editor saves — it changes its own copy of
    /// the list and hands over the whole order, so the edits land whatever
    /// order they were made in.
    ///
    /// # Errors
    /// No such list, or the server refused a change.
    pub async fn set_songs(&self, id: &str, songs: &[String]) -> eyre::Result<()> {
        let collections: CollectionServiceClient = self.client().await?;
        let fail = |what: &str, e: &dyn std::fmt::Debug| eyre::eyre!("{what}: {e:?}");
        let mut list = collections
            .get(id.to_owned())
            .await
            .map_err(|e| fail("reading the list", &e))?
            .ok_or_else(|| eyre::eyre!("no list {id}"))?;
        list.sort_items();
        let mut now: Vec<String> = list
            .items
            .iter()
            .filter(|i| i.node.kind == NodeKind::Song)
            .map(|i| i.node.id.clone())
            .collect();
        for gone in now.clone().iter().filter(|s| !songs.contains(s)) {
            collections
                .remove_item(id.to_owned(), NodeRef::song(gone.clone()))
                .await
                .map_err(|e| fail("removing a song", &e))?;
            now.retain(|s| s != gone);
        }
        for (at, slug) in songs.iter().enumerate() {
            if now.get(at) == Some(slug) {
                continue;
            }
            // After the song before it; the first, after nothing — which
            // Task reads as the end, so the first is put first by moving
            // everything else after it.
            let after = at.checked_sub(1).map(|i| NodeRef::song(songs[i].clone()));
            let placement = Placement {
                collection_id: id.to_owned(),
                node: NodeRef::song(slug.clone()),
                after: after.clone(),
            };
            if now.contains(slug) {
                collections
                    .reorder(placement)
                    .await
                    .map_err(|e| fail("moving a song", &e))?;
            } else {
                collections
                    .add_item(placement)
                    .await
                    .map_err(|e| fail("adding a song", &e))?;
            }
            now.retain(|s| s != slug);
            match &after {
                Some(_) => now.insert(at.min(now.len()), slug.clone()),
                None => {
                    // `slug` went to the end: bring every other song after it.
                    now.push(slug.clone());
                    let mut last = slug.clone();
                    for other in now.clone().iter().filter(|s| *s != slug) {
                        collections
                            .reorder(Placement {
                                collection_id: id.to_owned(),
                                node: NodeRef::song(other.clone()),
                                after: Some(NodeRef::song(last.clone())),
                            })
                            .await
                            .map_err(|e| fail("moving a song", &e))?;
                        last.clone_from(other);
                    }
                    now.retain(|s| s != slug);
                    now.insert(0, slug.clone());
                }
            }
        }
        Ok(())
    }

    /// Download a song's session into `into` (created), returning the
    /// folder: `into/<slug>/…`, the same layout it was uploaded from. A
    /// file already there at the same size is left alone.
    ///
    /// What Session plays from comes down — the `.RPP`, the `.session`,
    /// the `.lrc`, `Media/Proxies/`, `Media/Peaks/` — and the originals in `Media/` only
    /// with `originals`: a set streams from its proxies, and the WAVs are
    /// ten times the size.
    ///
    /// # Errors
    /// No session root for the song, or a transfer failed twice.
    pub async fn pull_session(
        &self,
        song: &str,
        into: &Path,
        originals: bool,
    ) -> eyre::Result<PathBuf> {
        let TaskSong {
            files,
            root,
            entries,
            ..
        } = self.song(song).await?;
        let mut files = files.lock().await.1.clone();
        let dest = into.join(song);
        for (rel, size) in entries {
            // Proxies and the waveform caches are what a set plays and
            // draws from; everything else in Media/ is an original.
            if is_original(&rel) && !originals {
                continue;
            }
            let local = dest.join(&rel);
            if std::fs::metadata(&local).is_ok_and(|meta| meta.len() == size) {
                continue;
            }
            if let Some(parent) = local.parent() {
                std::fs::create_dir_all(parent)?;
            }
            // Streamed to disk, never held whole; one retry on a fresh
            // connection, which is what a dropped stream needs.
            if let Err(first) = fetch_to(&files, root, &rel, &local).await {
                files = self.files().await?;
                fetch_to(&files, root, &rel, &local)
                    .await
                    .map_err(|e| eyre::eyre!("fetching {rel}: {e} (and before that: {first})"))?;
            }
        }
        self.pull_chart(song, &dest).await?;
        Ok(dest)
    }

    /// A song's session where it lies in the library: its files listed
    /// (the `.RPP` first — what a client opens; the prepared `.session`
    /// beside it opens instead), readable by range. What a client that
    /// streams the song in reads from, rather than pulling it whole.
    ///
    /// # Errors
    ///
    /// The server cannot be reached, or the song has no session root.
    pub async fn song(&self, song: &str) -> eyre::Result<TaskSong> {
        let files = self.files().await?;
        let dir = session_root_dir(song);
        let roots = files
            .roots
            .list()
            .await
            .map_err(|e| eyre::eyre!("listing roots: {e:?}"))?;
        let root = roots
            .iter()
            .find(|r| {
                r.path
                    .as_deref()
                    .is_some_and(|p| p.trim_end_matches('/').ends_with(&dir))
            })
            .ok_or_else(|| eyre::eyre!("no session for song:{song} (no `{dir}` root)"))?;
        let id = files_client::root_id(root);
        let mut entries = Vec::new();
        let mut pending = vec![String::new()];
        while let Some(folder) = pending.pop() {
            let path = RootPath::parse(&folder).map_err(|e| eyre::eyre!("path {folder}: {e:?}"))?;
            let listed = files
                .tree
                .browse(id, path)
                .await
                .map_err(|e| eyre::eyre!("browsing {folder}: {e:?}"))?;
            for entry in listed {
                let rel = if folder.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{folder}/{}", entry.name)
                };
                if entry.is_dir {
                    pending.push(rel);
                } else {
                    entries.push((rel, entry.size.unwrap_or(0)));
                }
            }
        }
        // The project first: the top-level `.RPP`.
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        if let Some(at) = entries
            .iter()
            .position(|(rel, _)| !rel.contains('/') && rel.to_lowercase().ends_with(".rpp"))
        {
            let project = entries.remove(at);
            entries.insert(0, project);
        }
        Ok(TaskSong {
            slug: song.to_owned(),
            library: self.clone(),
            files: std::sync::Arc::new(tokio::sync::Mutex::new((0, files))),
            root: id,
            entries,
        })
    }

    /// The song's main chart (its keyflow source) from the library — THE
    /// chart: it replaces whatever `.kf` the session's files carried.
    /// `None` when the song has none.
    ///
    /// # Errors
    ///
    /// The server cannot be reached, or refused.
    pub async fn song_chart(&self, song: &str) -> eyre::Result<Option<String>> {
        let resources: ResourcesServiceClient = self.client().await?;
        let charts = resources
            .list_charts(format!("song:{song}"))
            .await
            .map_err(|e| eyre::eyre!("charts of song:{song}: {e:?}"))?;
        let Some(main) = charts.iter().find(|c| c.is_default).or(charts.first()) else {
            return Ok(None);
        };
        let chart = resources
            .chart(main.slug.clone())
            .await
            .map_err(|e| eyre::eyre!("chart:{}: {e:?}", main.slug))?;
        Ok(Some(chart.source))
    }

    /// The song's main chart, written beside its `.RPP` as `<Song>.kf` —
    /// where the app looks for it. The chart lives in the library, not in
    /// the session's files: one chart, the same one keyflow edits.
    async fn pull_chart(&self, song: &str, dest: &Path) -> eyre::Result<()> {
        let Some(chart) = self.song_chart(song).await? else {
            return Ok(());
        };
        write_chart(dest, song, &chart)
    }

    /// Pull every song of a setlist and write a `.setlist` beside them
    /// that the app opens: `into/<Setlist>.setlist`.
    ///
    /// # Errors
    /// As [`Self::pull_session`].
    pub async fn pull_setlist(
        &self,
        setlist: &Setlist,
        into: &Path,
        originals: bool,
    ) -> eyre::Result<PathBuf> {
        std::fs::create_dir_all(into)?;
        let mut lines = vec!["# Pulled from the library. One song folder per line.".to_owned()];
        for song in &setlist.songs {
            self.pull_session(&song.slug, into, originals).await?;
            lines.push(song.slug.clone());
        }
        let file = into.join(format!("{}.setlist", setlist.title));
        std::fs::write(&file, lines.join("\n") + "\n")?;
        Ok(file)
    }
}

/// Write a song's library chart into its folder as `<Song>.kf` (the
/// `.RPP`'s stem), replacing any other `.kf` there — the app opens the one
/// `.kf` beside the song, and refuses to guess between two.
///
/// # Errors
///
/// The folder cannot be read or written.
pub fn write_chart(dest: &Path, song: &str, chart: &str) -> eyre::Result<()> {
    let files: Vec<PathBuf> = std::fs::read_dir(dest)?
        .flatten()
        .map(|e| e.path())
        .collect();
    for old in files
        .iter()
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("kf")))
    {
        std::fs::remove_file(old)?;
    }
    let stem = files
        .iter()
        .find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("rpp")))
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| song.to_owned());
    std::fs::write(dest.join(format!("{stem}.kf")), chart)?;
    Ok(())
}

/// Whether a song-root path is an original (the WAVs in `Media/`), not a
/// proxy or a waveform cache. Case-blind: macOS is, and a root uploaded
/// from it can carry `Media/peaks/` for the folder the app calls Peaks.
#[must_use]
pub fn is_original(rel: &str) -> bool {
    let lower = rel.to_ascii_lowercase();
    lower.starts_with("media/")
        && !lower.starts_with("media/proxies/")
        && !lower.starts_with("media/peaks/")
}

/// A song's session in the library, read where it lies (see
/// [`Library::song`]).
#[derive(Clone)]
pub struct TaskSong {
    /// The library's song id (`washed`).
    pub slug: String,
    library: Library,
    /// The connection reads go over, and how many times it has been
    /// replaced — so a dead one is redialled once, not by every read that
    /// found it dead.
    files: std::sync::Arc<tokio::sync::Mutex<(u64, FilesClient)>>,
    root: files_proto::id::RootId,
    /// Every file of the session root, with its size — the project first.
    pub entries: Vec<(String, u64)>,
}

/// How long one range read may take before its connection is taken for
/// dead — a stalled read would otherwise hold a fetch slot forever: a few
/// seconds, and a second more for each 100 KB asked for. A flat 20 s let
/// one stalled 200-byte read (a song has hundreds) hold a download up for
/// 20 s at a time; a small read that takes more than a few is not coming.
fn read_timeout(len: u64) -> std::time::Duration {
    std::time::Duration::from_secs(5) + std::time::Duration::from_millis(len / 100)
}

impl TaskSong {
    /// The bytes `range` of `path` in the song's session — a seek, not a
    /// download. A read that fails or stalls redials the library and is
    /// tried again, a few times: a streamed song outlives any one
    /// connection.
    ///
    /// # Errors
    ///
    /// The server refused, or the file is not there, every time.
    pub async fn read(&self, path: &str, range: std::ops::Range<u64>) -> eyre::Result<Vec<u8>> {
        /// Tries at one read, each after the last on a fresh connection: the
        /// server's byte streams sometimes stall (no answer, or closed
        /// before done), and a download reads hundreds of small files — one
        /// stall in them lost a whole song when a read had two tries.
        const TRIES: u32 = 4;
        if range.is_empty() {
            return Ok(Vec::new());
        }
        let mut errors = Vec::new();
        for tried in 0..TRIES {
            let (generation, files) = self.files.lock().await.clone();
            match self.read_on(&files, path, &range).await {
                Ok(bytes) => return Ok(bytes),
                Err(e) => errors.push(e.to_string()),
            }
            if tried + 1 == TRIES {
                break;
            }
            // Redialled once for everyone who found this connection dead,
            // not by every read that did.
            let mut held = self.files.lock().await;
            if held.0 == generation {
                match self.library.files().await {
                    Ok(fresh) => *held = (generation + 1, fresh),
                    Err(e) => errors.push(format!("redialling: {e}")),
                }
            }
            drop(held);
            architect::platform::sleep(std::time::Duration::from_millis(
                300 * u64::from(tried + 1),
            ))
            .await;
        }
        eyre::bail!("{}", errors.join(" (then) "))
    }

    /// Where many of the song's documents are read in one request (the
    /// server's `files/{root}/docs`: the paths in the body, one per line;
    /// a record per path back).
    #[must_use]
    pub fn docs_url(&self) -> String {
        format!(
            "{}/org/{}/files/{}/docs",
            self.library.http_base(),
            self.library.org,
            self.root.get()
        )
    }

    /// The token the server knows the signed-in person by, if any.
    #[must_use]
    pub fn token(&self) -> Option<&str> {
        self.library.token.as_deref()
    }

    /// The whole of `path`, streamed into `sink` a piece at a time — one
    /// request for the file, not one per range: how a download fetches a
    /// proxy. No timeout of its own (a proxy is tens of megabytes, and a
    /// slow phone may take minutes); a dropped connection ends it with an
    /// error. Returns the bytes read.
    ///
    /// # Errors
    ///
    /// The server refused, the file is not there, or the stream broke.
    ///
    /// Each on a connection of its own: on a shared one, once a whole-file
    /// stream had finished, the next ones started on it received nothing
    /// and never ended (three proxies came down; the next three sat at 0
    /// bytes for ten minutes). A dial per proxy costs far less than that.
    pub async fn read_to(&self, path: &str, sink: impl FnMut(&[u8])) -> eyre::Result<u64> {
        let files = self.library.files().await?;
        files
            .read_to(self.root, path, sink)
            .await
            .map_err(|e| eyre::eyre!("{path}: {e}"))
    }

    async fn read_on(
        &self,
        files: &FilesClient,
        path: &str,
        range: &std::ops::Range<u64>,
    ) -> eyre::Result<Vec<u8>> {
        let limit = read_timeout(range.end - range.start);
        architect::platform::timeout(
            limit,
            files.read_range(self.root, path, range.start, range.end - 1),
        )
        .await
        .map_err(|_| eyre::eyre!("{path} {range:?}: no answer in {:.0}s", limit.as_secs_f64()))?
        .map_err(|e| eyre::eyre!("{path} {range:?}: {e}"))
    }
}

/// Stream one file of a root to `local`, through a temporary name so a
/// broken transfer never leaves a file that looks complete.
async fn fetch_to(
    files: &FilesClient,
    root: files_proto::id::RootId,
    rel: &str,
    local: &Path,
) -> eyre::Result<()> {
    use std::io::Write as _;
    let partial = local.with_extension("part");
    let mut out = std::io::BufWriter::new(std::fs::File::create(&partial)?);
    let mut failed = None;
    files
        .read_to(root, rel, |chunk| {
            if failed.is_none()
                && let Err(e) = out.write_all(chunk)
            {
                failed = Some(e);
            }
        })
        .await
        .map_err(|e| eyre::eyre!("{e}"))?;
    if let Some(e) = failed {
        return Err(e.into());
    }
    out.flush()?;
    drop(out);
    std::fs::rename(&partial, local)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_title_slugs_as_the_library_does() {
        assert_eq!(
            super::slugify("God, I'm Just Grateful"),
            "god-im-just-grateful"
        );
        assert_eq!(super::slugify("Always On Time"), "always-on-time");
        assert_eq!(super::slugify("  Thank God I’m Free!"), "thank-god-im-free");
    }
}

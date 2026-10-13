//! One file per project, on Turso.
//!
//! The project is a Loro document — what everyone editing it, on any
//! device, merges — and this is where it lives on disk: one SQLite-format
//! file, written through [Turso](https://github.com/tursodatabase/turso),
//! holding
//!
//! ```text
//! meta       key → value              the schema, the project's id
//! blobs      hash → bytes (zstd)      plugin states, MIDI sources, an
//!                                     imported project's verbatim text —
//!                                     once each, however many branches
//!                                     or joined projects refer to them
//! branches   name → parent, base      the song (`main`) and its
//!                                     alternatives and checkpoints, each
//!                                     forked from another at a point
//! updates    seq → branch, update     each undo step's edits, appended
//!                                     to its branch (zstd)
//! snapshots  branch → upto, snapshot  a branch's doc as of its update
//!                                     `upto`, folded by [`Store::compact`]
//! ```
//!
//! A branch is Logic's alternative: `Organized` forked from the song when
//! the organizing was done, edited on its own after. It shares everything
//! before the fork with the song — the history up to there, every blob —
//! and keeps only what it did since.
//!
//! Media is not here: audio stays files beside the project, as it is now.
//!
//! What makes it small is what goes in, not the engine: a project as a
//! Loro doc rather than text, every blob stored once by content, and an
//! alternative a fork of one history rather than a copy of the project. Turso itself does not compress; the blobs and the history are
//! zstd'd here.
//!
//! Turso's API is async; the store drives it to completion on the
//! calling thread, so callers (a save, an open) stay synchronous.

use sha2::{Digest, Sha256};

/// The schema this build writes, kept in `meta.schema`.
pub const SCHEMA: i64 = 1;

/// The zstd level blobs and history are compressed at: well past the
/// default's ratio, still fast to write for a save.
const LEVEL: i32 = 9;

/// What went wrong.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database: {0}")]
    Db(#[from] turso::Error),
    #[error("compression: {0}")]
    Codec(#[from] std::io::Error),
    #[error("loro: {0}")]
    Loro(String),
    #[error("blob {0} is not in the store")]
    Missing(String),
    #[error("blob {0} does not hash to its name")]
    Corrupt(String),
    #[error("the store is schema {found}, this build reads {SCHEMA}")]
    Schema { found: i64 },
    #[error("no branch {0}")]
    NoBranch(String),
}

/// The song's own branch: what it is when nothing else is named.
pub const MAIN: &str = "main";

/// The store's results.
pub type StoreResult<T> = Result<T, StoreError>;

/// A blob's name: its SHA-256, lowercase hex — the same names the
/// `.session` folder's `objects/` use, so a project moves between them
/// without renaming anything.
#[must_use]
pub fn hash_of(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// A branch: the song, or an alternative or checkpoint of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    /// The branch it was forked from; `None` for the song itself.
    pub parent: Option<String>,
    /// Seconds since the epoch it was made at.
    pub at: i64,
}

/// One step of a branch's history, as [`Store::steps`] lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// The undo step's name.
    pub label: String,
    /// When it was written, in seconds since the epoch.
    pub at: i64,
    /// What it keeps, packed.
    pub bytes: i64,
}

/// One project's file.
pub struct Store {
    conn: turso::Connection,
    /// Kept so the file stays open as long as the store.
    _db: turso::Database,
}

/// Run `future` to completion here.
fn run<T>(future: impl std::future::Future<Output = T>) -> T {
    pollster::block_on(future)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

fn import(doc: &loro::LoroDoc, packed: &[u8]) -> StoreResult<()> {
    let bytes = zstd::decode_all(packed)?;
    doc.import(&bytes)
        .map_err(|e| StoreError::Loro(e.to_string()))?;
    Ok(())
}

impl Store {
    /// Open the project file at `path`, made if it is not there.
    ///
    /// # Errors
    ///
    /// The file would not open, or it is a schema this build does not
    /// read.
    pub fn open(path: &std::path::Path) -> StoreResult<Self> {
        run(async {
            // Several processes at once — the app saving it, a script or
            // another tool reading it — through a shared write-ahead log.
            let db = turso::Builder::new_local(&path.to_string_lossy())
                .experimental_multiprocess_wal(true)
                .build()
                .await?;
            let conn = db.connect()?;
            for sql in [
                "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
                "CREATE TABLE IF NOT EXISTS blobs (hash TEXT PRIMARY KEY, kind TEXT NOT NULL, \
                 size INTEGER NOT NULL, data BLOB NOT NULL)",
                "CREATE TABLE IF NOT EXISTS branches (name TEXT PRIMARY KEY, parent TEXT, \
                 base BLOB, at INTEGER NOT NULL)",
                "CREATE TABLE IF NOT EXISTS updates (seq INTEGER PRIMARY KEY AUTOINCREMENT, \
                 branch TEXT NOT NULL, label TEXT NOT NULL, at INTEGER NOT NULL, \
                 data BLOB NOT NULL)",
                "CREATE TABLE IF NOT EXISTS snapshots (branch TEXT PRIMARY KEY, \
                 upto INTEGER NOT NULL, at INTEGER NOT NULL, data BLOB NOT NULL)",
            ] {
                conn.execute(sql, ()).await?;
            }
            conn.execute(
                "INSERT INTO branches (name, parent, base, at) VALUES (?1, NULL, NULL, ?2) \
                 ON CONFLICT(name) DO NOTHING",
                (MAIN, now()),
            )
            .await?;
            let store = Self { conn, _db: db };
            match store.meta_async("schema").await? {
                None => {
                    store.set_meta_async("schema", &SCHEMA.to_string()).await?;
                }
                Some(found) => {
                    let found = found.parse().unwrap_or(-1);
                    if found != SCHEMA {
                        return Err(StoreError::Schema { found });
                    }
                }
            }
            Ok(store)
        })
    }

    async fn meta_async(&self, key: &str) -> StoreResult<Option<String>> {
        let mut rows = self
            .conn
            .query("SELECT value FROM meta WHERE key = ?1", (key,))
            .await?;
        Ok(match rows.next().await? {
            Some(row) => row.get::<String>(0).ok(),
            None => None,
        })
    }

    async fn set_meta_async(&self, key: &str, value: &str) -> StoreResult<()> {
        self.conn
            .execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                (key, value),
            )
            .await?;
        Ok(())
    }

    /// A `meta` value.
    ///
    /// # Errors
    ///
    /// The database refused the read.
    pub fn meta(&self, key: &str) -> StoreResult<Option<String>> {
        run(self.meta_async(key))
    }

    /// Set a `meta` value.
    ///
    /// # Errors
    ///
    /// The database refused the write.
    pub fn set_meta(&self, key: &str, value: &str) -> StoreResult<()> {
        run(self.set_meta_async(key, value))
    }

    /// Fold what is written into the file itself: the engine writes ahead
    /// into a log beside it (`Song.session-wal`), and a project is one
    /// file to copy, sync or send only once that log is emptied. Done after
    /// a save.
    ///
    /// # Errors
    ///
    /// The database refused.
    pub fn checkpoint(&self) -> StoreResult<()> {
        run(async {
            let mut rows = self
                .conn
                .query("PRAGMA wal_checkpoint(TRUNCATE)", ())
                .await?;
            while rows.next().await?.is_some() {}
            StoreResult::Ok(())
        })
    }

    /// Keep `bytes` (of `kind`: `fx`, `midi`, `rpp`, …) and say its name.
    /// Bytes already here are not written again: a blob is stored once
    /// however many branches or joined projects use it.
    ///
    /// # Errors
    ///
    /// The database refused the write.
    pub fn put_blob(&self, kind: &str, bytes: &[u8]) -> StoreResult<String> {
        let hash = hash_of(bytes);
        let packed = zstd::encode_all(bytes, LEVEL)?;
        let size = i64::try_from(bytes.len()).unwrap_or(i64::MAX);
        run(self.conn.execute(
            "INSERT INTO blobs (hash, kind, size, data) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(hash) DO NOTHING",
            (hash.as_str(), kind, size, packed),
        ))?;
        Ok(hash)
    }

    /// The blob named `hash`, checked against its name.
    ///
    /// # Errors
    ///
    /// It is not here, or what is here is not what the name says.
    pub fn blob(&self, hash: &str) -> StoreResult<Vec<u8>> {
        let packed: Vec<u8> = run(async {
            let mut rows = self
                .conn
                .query("SELECT data FROM blobs WHERE hash = ?1", (hash,))
                .await?;
            match rows.next().await? {
                Some(row) => Ok(row.get::<Vec<u8>>(0)?),
                None => Err(StoreError::Missing(hash.to_owned())),
            }
        })?;
        let bytes = zstd::decode_all(packed.as_slice())?;
        if hash_of(&bytes) != hash {
            return Err(StoreError::Corrupt(hash.to_owned()));
        }
        Ok(bytes)
    }

    /// How many blobs, and their bytes before and after compression.
    ///
    /// # Errors
    ///
    /// The database refused the read.
    pub fn blob_totals(&self) -> StoreResult<(i64, i64, i64)> {
        run(async {
            let mut rows = self
                .conn
                .query(
                    "SELECT COUNT(*), COALESCE(SUM(size), 0), COALESCE(SUM(LENGTH(data)), 0) \
                     FROM blobs",
                    (),
                )
                .await?;
            Ok(match rows.next().await? {
                Some(row) => (row.get(0)?, row.get(1)?, row.get(2)?),
                None => (0, 0, 0),
            })
        })
    }

    /// Every branch: the song first, then its alternatives oldest first.
    ///
    /// # Errors
    ///
    /// The database refused the read.
    pub fn branches(&self) -> StoreResult<Vec<Branch>> {
        run(async {
            let mut rows = self
                .conn
                .query(
                    "SELECT name, parent, at FROM branches \
                     ORDER BY (name = ?1) DESC, at, name",
                    (MAIN,),
                )
                .await?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().await? {
                out.push(Branch {
                    name: row.get(0)?,
                    parent: row.get::<Option<String>>(1)?,
                    at: row.get(2)?,
                });
            }
            Ok(out)
        })
    }

    /// Fork `name` from `parent` at `doc`'s current state (`doc` being
    /// `parent` as loaded, with whatever it holds unsaved appended first by
    /// the caller). A branch of that name already here is replaced.
    ///
    /// # Errors
    ///
    /// The database refused the write.
    pub fn fork(&self, name: &str, parent: &str, doc: &loro::LoroDoc) -> StoreResult<()> {
        let base = doc.state_frontiers().encode();
        run(async {
            self.conn.execute("BEGIN", ()).await?;
            for sql in [
                "DELETE FROM updates WHERE branch = ?1",
                "DELETE FROM snapshots WHERE branch = ?1",
                "DELETE FROM branches WHERE name = ?1",
            ] {
                self.conn.execute(sql, (name,)).await?;
            }
            self.conn
                .execute(
                    "INSERT INTO branches (name, parent, base, at) VALUES (?1, ?2, ?3, ?4)",
                    (name, parent, base, now()),
                )
                .await?;
            self.conn.execute("COMMIT", ()).await?;
            StoreResult::Ok(())
        })
    }

    /// Forget branch `name` and what it did; the song itself cannot go.
    ///
    /// # Errors
    ///
    /// The database refused the write.
    pub fn delete_branch(&self, name: &str) -> StoreResult<()> {
        if name == MAIN {
            return Err(StoreError::NoBranch(name.to_owned()));
        }
        run(async {
            for sql in [
                "DELETE FROM updates WHERE branch = ?1",
                "DELETE FROM snapshots WHERE branch = ?1",
                "DELETE FROM branches WHERE name = ?1",
            ] {
                self.conn.execute(sql, (name,)).await?;
            }
            StoreResult::Ok(())
        })
    }

    /// Append one step of edits — a Loro update, `label` the undo step's
    /// name — to `branch`.
    ///
    /// # Errors
    ///
    /// The database refused the write.
    pub fn append_update(&self, branch: &str, label: &str, update: &[u8]) -> StoreResult<()> {
        let packed = zstd::encode_all(update, LEVEL)?;
        run(self.conn.execute(
            "INSERT INTO updates (branch, label, at, data) VALUES (?1, ?2, ?3, ?4)",
            (branch, label, now(), packed),
        ))?;
        Ok(())
    }

    /// `branch`'s own steps since its last compaction, oldest first: each
    /// one's name, when, and how many bytes it keeps.
    ///
    /// # Errors
    ///
    /// The database refused the read.
    pub fn steps(&self, branch: &str) -> StoreResult<Vec<Step>> {
        run(async {
            let mut rows = self
                .conn
                .query(
                    "SELECT label, at, length(data) FROM updates WHERE branch = ?1 ORDER BY seq",
                    (branch,),
                )
                .await?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().await? {
                out.push(Step {
                    label: row.get(0)?,
                    at: row.get(1)?,
                    bytes: row.get(2)?,
                });
            }
            Ok(out)
        })
    }

    /// `branch`'s doc: its snapshot if it has one, else its parent forked
    /// where it forked; then every update of its own after that.
    ///
    /// # Errors
    ///
    /// No such branch, the database refused the read, or the history
    /// would not import.
    pub fn load(&self, branch: &str) -> StoreResult<loro::LoroDoc> {
        let (parent, base): (Option<String>, Option<Vec<u8>>) = run(async {
            let mut rows = self
                .conn
                .query(
                    "SELECT parent, base FROM branches WHERE name = ?1",
                    (branch,),
                )
                .await?;
            match rows.next().await? {
                Some(row) => Ok((row.get(0)?, row.get(1)?)),
                None => Err(StoreError::NoBranch(branch.to_owned())),
            }
        })?;
        let snapshot: Option<(i64, Vec<u8>)> = run(async {
            let mut rows = self
                .conn
                .query(
                    "SELECT upto, data FROM snapshots WHERE branch = ?1",
                    (branch,),
                )
                .await?;
            StoreResult::Ok(match rows.next().await? {
                Some(row) => Some((row.get(0)?, row.get(1)?)),
                None => None,
            })
        })?;
        let (doc, upto) = match (snapshot, parent, base) {
            (Some((upto, packed)), _, _) => {
                let doc = loro::LoroDoc::new();
                import(&doc, &packed)?;
                (doc, upto)
            }
            (None, Some(parent), Some(base)) => {
                let from = self.load(&parent)?;
                let at =
                    loro::Frontiers::decode(&base).map_err(|e| StoreError::Loro(e.to_string()))?;
                (
                    from.fork_at(&at)
                        .map_err(|e| StoreError::Loro(e.to_string()))?,
                    0,
                )
            }
            _ => (loro::LoroDoc::new(), 0),
        };
        let updates: Vec<Vec<u8>> = run(async {
            let mut rows = self
                .conn
                .query(
                    "SELECT data FROM updates WHERE branch = ?1 AND seq > ?2 ORDER BY seq",
                    (branch, upto),
                )
                .await?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().await? {
                out.push(row.get::<Vec<u8>>(0)?);
            }
            StoreResult::Ok(out)
        })?;
        for packed in updates {
            import(&doc, &packed)?;
        }
        Ok(doc)
    }

    /// Fold `branch`'s history into one snapshot of `doc` (which must be
    /// the branch as loaded, with every update appended so far) and drop
    /// the updates it covers. History is kept — a Loro snapshot carries
    /// every operation — so a branch forked from this one still finds its
    /// fork point.
    ///
    /// # Errors
    ///
    /// The doc would not export, or the database refused the write.
    pub fn compact(&self, branch: &str, doc: &loro::LoroDoc) -> StoreResult<()> {
        let snapshot = doc
            .export(loro::ExportMode::Snapshot)
            .map_err(|e| StoreError::Loro(e.to_string()))?;
        let packed = zstd::encode_all(snapshot.as_slice(), LEVEL)?;
        run(async {
            let mut rows = self
                .conn
                .query(
                    "SELECT COALESCE(MAX(seq), 0) FROM updates WHERE branch = ?1",
                    (branch,),
                )
                .await?;
            let upto: i64 = match rows.next().await? {
                Some(row) => row.get(0)?,
                None => 0,
            };
            drop(rows);
            self.conn.execute("BEGIN", ()).await?;
            self.conn
                .execute(
                    "INSERT INTO snapshots (branch, upto, at, data) VALUES (?1, ?2, ?3, ?4) \
                     ON CONFLICT(branch) DO UPDATE SET upto = excluded.upto, \
                     at = excluded.at, data = excluded.data",
                    (branch, upto, now(), packed),
                )
                .await?;
            self.conn
                .execute(
                    "DELETE FROM updates WHERE branch = ?1 AND seq <= ?2",
                    (branch, upto),
                )
                .await?;
            self.conn.execute("COMMIT", ()).await?;
            StoreResult::Ok(())
        })
    }
}

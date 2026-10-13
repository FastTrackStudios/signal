# Files platform — Rust crate landscape (storage, sync, watching)

Research for [#226](https://github.com/FastTrackStudios/FastTrackStudio/issues/226)
(part of #224, blocking #228). Surveyed 2026-08-11 against each crate's own
crates.io metadata, docs.rs, and repository (links inline). Workspace context
checked against the root `Cargo.lock` of this monorepo.

**Scope**: the Files platform needs (a) chunking + dedup for multi-GB
audio/video versioning, (b) a content-addressed local store, (c) watching
project roots on a NAS (NFS4 client mounts of a snapraid-backed server),
(d) a selective-sync laptop daemon, (e) a WebDAV compat bridge, (f) a
placement-aware storage layer over S3/local/external drives, (g) possibly a
virtual mount. Everything must sit on tokio (the workspace standard).

---

## 1. Content-defined chunking + hashing

### `fastcdc` — RECOMMENDED

- **What**: pure-Rust FastCDC content-defined chunking; three variants
  (`v2016`, `v2020` "rolling two bytes", legacy `ronomon`), non-streaming
  (works with `memmap2`), `StreamCDC` over `Read`, and `AsyncStreamCDC`
  feature-gated for **tokio** or futures
  ([repo](https://github.com/nlfiedler/fastcdc-rs)).
- **Maintenance**: v4.0.1 released 2026-04-26 (4.0.0 on 2026-04-11; 3.2.x in
  2025) — steady annual releases by nlfiedler, 1.3M downloads
  ([crates.io](https://crates.io/crates/fastcdc)).
- **License**: MIT. **Guarantee**: deterministic — "returns exactly the same
  results for the same input" (README), which is what dedup needs.
- **Verdict**: the clear choice. The v2020 variant with normalized chunking is
  the standard for dedup stores; the tokio `AsyncStreamCDC` fits our daemons
  directly. Alternatives are effectively dead: `gearhash` 0.1.3 (SIMD gear
  hash, last release 2020-04, [crates.io](https://crates.io/crates/gearhash))
  and `cdchunking` 1.0.1 (last release 2020-08,
  [crates.io](https://crates.io/crates/cdchunking)).

### `blake3` — RECOMMENDED (already in workspace)

- **What**: the official BLAKE3-team hash crate
  ([repo](https://github.com/BLAKE3-team/BLAKE3)); SIMD everywhere, rayon
  multithreading, and the tree structure that makes bao verified streaming
  possible.
- **Maintenance**: 1.8.6 released 2026-08-05
  ([crates.io](https://crates.io/crates/blake3)). **License**:
  CC0-1.0 OR Apache-2.0 OR Apache-2.0-with-LLVM-exception.
- **Workspace**: already locked at 1.8.5 (root `Cargo.lock`), pulled in by the
  iroh stack. Using blake3 as the content hash keeps us wire-compatible with
  iroh-blobs (its `Hash` *is* a blake3 hash).
- **Verdict**: use it; it is the content-address hash iroh-blobs mandates
  anyway, so chunk-level and blob-level identity share one primitive.

---

## 2. Content-addressed store

### `iroh-blobs` — RECOMMENDED with eyes open

- **What**: content-addressed blob store + transfer protocol over BLAKE3
  verified streaming (bao-tree 0.16)
  ([docs.rs](https://docs.rs/iroh-blobs/latest/iroh_blobs/)).
- **Local use without networking**: yes. `MemStore` and `FsStore`
  (`fs-store` feature, default) are usable purely locally; the `api::Store`
  entry point works in-process, with `api::blobs` for local blob interaction
  and persistent `Tag`s / in-memory `TempTag`s protecting content
  ([api module](https://docs.rs/iroh-blobs/latest/iroh_blobs/api/index.html)).
  Networking (`BlobsProtocol`, `provider`, `get`, `downloader`) is a layer on
  top that requires an iroh `Endpoint`/`Router` — we can adopt the store now
  and light up P2P/verified-transfer later.
- **GC**: real and configurable — `store::GcConfig { interval: Duration,
  add_protected: Option<ProtectCb> }`; the callback receives a
  `HashSet<Hash>` to protect per run and can `Continue`/`Abort`; tags and
  `Batch::temp_tag` protect blobs during long writes
  ([GcConfig](https://docs.rs/iroh-blobs/latest/iroh_blobs/store/struct.GcConfig.html)).
- **FsStore internals**: redb (^4.1) metadata DB, inlining options for small
  data/outboards, explicit `Store::shutdown` for durability; docs warn the
  store "might miss some writes in the last seconds before shutdown" without
  it, and that large partial blobs cause startup delay
  ([fs store docs](https://docs.rs/iroh-blobs/latest/iroh_blobs/store/fs/index.html)).
- **Maintenance**: 0.103.0 released 2026-06-15, roughly monthly releases
  ([crates.io](https://crates.io/crates/iroh-blobs)). MSRV 1.91. **License**:
  MIT OR Apache-2.0.
- **Caveats**: the README still carries "this version of iroh-blobs is not yet
  considered production quality. For now, if you need production quality, use
  iroh-blobs 0.35" ([repo](https://github.com/n0-computer/iroh-blobs)) — that
  note predates the 0.10x series' stabilization work but has not been
  retracted; treat the store as beta and keep our own chunk manifest so the
  store is rebuildable.
- **Workspace fit**: excellent — root `Cargo.toml` already pins `iroh = "1"`
  (locked 1.0.2), and iroh-blobs 0.103.0's manifest depends on **iroh 1.0.0**
  ([Cargo.toml](https://github.com/n0-computer/iroh-blobs/blob/main/Cargo.toml)),
  so no version split.
- **Verdict**: adopt as the CAS layer. Note iroh-blobs dedups at *whole-blob*
  granularity (one hash per blob); for multi-GB session files with small
  edits, chunk with fastcdc first and store chunks (or chunk manifests +
  `HashSeq`) as blobs — that combination gives content-level dedup plus
  bao-verified streaming per chunk.

---

## 3. Filesystem watching

### `notify` — RECOMMENDED for local trees; NOT sufficient on the NAS mount

- **What**: the de-facto cross-platform watcher (inotify / FSEvents / kqueue /
  ReadDirectoryChangesW / `PollWatcher` fallback), used by cargo-watch,
  rust-analyzer, deno, watchexec, zed
  ([repo](https://github.com/notify-rs/notify)). Debouncers:
  `notify-debouncer-mini` / `notify-debouncer-full`.
- **Maintenance**: 8.2.0 released 2026-06-15 (already in our `Cargo.lock` as a
  transitive dep); MSRV 1.88. **License**: notify core CC0-1.0; the
  types/debouncer/file-id crates MIT OR Apache-2.0.
- **Known problems (their own docs)**: "Network mounted filesystems like NFS
  may not emit any events for notify to listen to", with `PollWatcher` as the
  documented workaround; also inotify watch-count sysctl limits and
  large-directory reliability caveats
  ([docs.rs known problems](https://docs.rs/notify/latest/notify/)).
- **Workspace gotcha**: the monorepo has a *local* crate literally named
  `notify` (`features/task/notify/notify`, mapped in root
  `[workspace.dependencies]`). Depending on crates.io notify from a Files
  crate requires a rename, e.g.
  `notify-fs = { package = "notify", version = "8" }`.

### The hard question: inotify on an NFS client

**Inotify will NOT fire on an NFS client for writes made by other machines.**
This is by design, not a bug:

- inotify(7) man page, "Limitations and caveats": *"Inotify reports only
  events that a user-space program triggers through the filesystem API. As a
  result, it does not catch remote events that occur on network filesystems.
  (Applications must fall back to polling the filesystem to catch such
  events.)"*
  ([man7.org](https://man7.org/linux/man-pages/man7/inotify.7.html)).
- The fsnotify hooks live in the local kernel's VFS write paths; an NFS
  *server* never tells clients about changes, because Linux has no mechanism
  to register remote watches — LWN's "Change notifications for network
  filesystems" (May 2022) covers the proposal to add such hooks (Steve
  French, for SMB/NFS/Ceph) and it has not landed
  ([LWN 896055](https://lwn.net/Articles/896055/)).
- Corollary that matters for us: inotify on the NFS mount *does* fire for
  writes made **through that same client** (they go through the local VFS),
  which makes partial coverage deceptive in testing. Writes from the studio
  machines to the NAS will be invisible to a watcher on any *other* machine.

**Server-side options, in order of preference:**

1. **Run the watcher daemon on the storage server itself** (planned anyway).
   On the server the exports are local filesystems, so inotify — i.e. plain
   `notify` — sees everything, including writes arriving via nfsd (nfsd goes
   through the local VFS). This is the architecture Files already assumes
   (server-side daemon watching project roots) and it is the correct one.
2. **fanotify** on the server for filesystem-wide watching: Linux 4.20's
   `FAN_MARK_FILESYSTEM` watches a whole filesystem regardless of mount, and
   5.1's `FAN_REPORT_FID` adds create/delete/move/attrib events
   ([kernel admin guide](https://www.kernel.org/doc/html/latest/admin-guide/filesystem-monitoring.html),
   [fanotify(7) via libc PR](https://github.com/rust-lang/libc/pull/1699)).
   Rust support: `nix::sys::fanotify`
   ([docs.rs](https://docs.rs/nix/latest/nix/sys/fanotify/index.html)) is the
   maintained binding (FID-mode ergonomics still under discussion,
   [nix#2486](https://github.com/nix-rust/nix/issues/2486)); the standalone
   `fanotify-rs` crate exists but is low-traffic. Needs CAP_SYS_ADMIN.
   Worth it only if per-directory inotify watch counts become a problem;
   start with notify/inotify.
3. **Polling scanners** as the reconciliation layer: notify's `PollWatcher`
   (optionally `compare_contents`) or our own periodic stat-scan diffing
   against the version index. Regardless of watcher choice we need this
   anyway — every watcher (inotify included) drops events on overflow, so the
   design should be "watcher = low-latency hint, scanner = source of truth".
4. **Watchman** (`watchman_client`, Meta's official crate, MIT,
   [crates.io](https://crates.io/crates/watchman_client)): solid
   subscription/query API, but last release 0.9.0 was 2024-06-18 and it
   requires running the Watchman C++ service; on NFS Watchman itself falls
   back to crawling. Verdict: skip — an extra non-Rust service for no
   capability we can't get from notify + our own scanner.

---

## 4. Sync engines

### Syncthing protocol in Rust: nothing usable

- The `syncthing` crate (0.6.0, 2025-09-21) is a **REST client for a running
  Syncthing daemon**, not a protocol implementation
  ([crates.io](https://crates.io/crates/syncthing)).
- `Syrus` implements only Block Exchange Protocol v1 of four planned
  protocols, self-labeled "Currently in Development", 13 stars, MIT
  ([repo](https://github.com/wesrer/Syrus)).
- `st-rust` is explicitly a toy/educational implementation
  ([repo](https://github.com/abusch/st-rust)).
- **Verdict**: no production-grade Syncthing-protocol crate exists. If we want
  Syncthing interop, drive a real Syncthing via its REST API; but for our own
  selective-sync daemon the better path is iroh-blobs verified transfer over
  the iroh stack we already ship (pack distribution already proves this
  pattern in-tree).

### Delta/rsync-family crates

- `fast_rsync` (Dropbox): optimized pure-Rust librsync-compatible
  signature/delta/apply — MD4-based, useful for rsync interop, not for a
  blake3 CAS; activity is sparse
  ([repo](https://github.com/dropbox/fast_rsync)).
- `librsync` (bindings) and `rdiff-rs` — the latter's repo was archived
  2024-03 ([repo](https://github.com/sourcefrog/rdiff-rs)).
- `syncfast`: rsync/rdiff/zsync clone, experimental
  ([repo](https://github.com/remram44/syncfast)).
- `librclone` (0.9.0, 2025-02-12, MIT OR Apache-2.0 OR CC0): FFI onto the Go
  rclone library — brings the whole Go runtime into our binary
  ([crates.io](https://crates.io/crates/librclone)). Fine as an ops escape
  hatch (bulk seeding external drives), wrong as an architectural component.
- **Verdict**: don't build sync on any of these. FastCDC chunking + blake3
  content addresses already give us delta transfer for free ("send the chunks
  the other side lacks"), which is the model iroh-blobs formalizes.

---

## 5. WebDAV server

### `dav-server` — RECOMMENDED

- **What**: the maintained fork of `webdav-handler`; async RFC4918 handler
  mapping WebDAV onto a filesystem abstraction. Passes Litmus basic, copymove,
  props, locks, and http suites; `LocalFs`/`MemFs` backends plus a
  `DavFileSystem` trait for custom backends (which is exactly where our
  version-store-backed view plugs in); `MemLs`/`FakeLs` lock managers (FakeLs
  is what makes Windows/macOS clients mount read-write); adapters for hyper,
  warp, actix-web, and **axum** — matching our `architect::axum_ws` server
  stack ([docs.rs](https://docs.rs/dav-server/latest/dav_server/)).
- **Maintenance**: 0.11.0 released 2026-02-21 (0.9/0.10 in Jan 2026, 0.8 in
  2025) by messense, 518k downloads
  ([crates.io](https://crates.io/crates/dav-server)). **License**: Apache-2.0.
- **Verdict**: the only serious option in the ecosystem, and a good one.
  Implement `DavFileSystem` over the Files project/version model rather than
  exposing `LocalFs` directly.

---

## 6. Object storage abstraction

### `object_store` (Apache Arrow) vs `opendal` (Apache OpenDAL)

| | `object_store` | `opendal` |
|---|---|---|
| Latest | 0.14.1, 2026-07-15 ([docs.rs](https://docs.rs/object_store/latest/object_store/)) | 0.58.1, 2026-07-31 ([crates.io](https://crates.io/crates/opendal)) |
| License | MIT OR Apache-2.0 | Apache-2.0 |
| Governance | Apache Arrow project (ex-InfluxData); used by crates.io, InfluxDB IOx | Apache Software Foundation top-level project, 6 active Rust maintainers |
| Backends | S3, GCS, Azure, LocalFileSystem, InMemory, HTTP/WebDAV | 50+: those plus SFTP/FTP/WebDAV/Google Drive/Redis/etc. |
| API shape | One `ObjectStore` trait: ranged `get`, conditional `put` (preconditions/optimistic concurrency), native multipart, vectored I/O, list-with-prefix, throttling adapters | `Operator` with `read/write/stat/list/delete`; cross-cutting `Layers` (retry, timeout, logging, metrics, tracing) |
| Runtime | tokio-first | tokio-first ("async-first, built on Tokio"), MSRV 1.91 |

- **Verdict**: **`object_store`** for the placement-aware storage layer. Its
  trait is small, stable, and semantics-precise (conditional puts and ranged
  reads matter for a chunk store; iroh-blobs-style verified ranges map onto
  ranged `get`), and it's the same abstraction crates.io itself runs on. Our
  backend set (S3, NAS paths, external drives) is fully covered, and it is
  **already in our lockfile** (0.12.5/0.13.2 via transitive deps). `opendal`
  earns its 50-backend breadth with a heavier surface; choose it only if the
  roadmap ever demands consumer-drive backends (Google Drive/Dropbox). The
  two are trait-compatible enough that a later swap behind our own
  `StorageLocation` trait is cheap — which is the real recommendation: keep
  the placement layer ours, implement it over `object_store` first.

---

## 7. FUSE

### `fuser` — VIABLE, keep for a later phase

- **What**: FUSE filesystems in userspace Rust; a rewrite of the libfuse C
  library rather than bindings — pure Rust except mount/umount, libfuse
  optional, FUSE 2 + 3 ABIs, Linux/FreeBSD (macOS untested)
  ([repo](https://github.com/cberner/fuser)).
- **Maintenance**: 0.18.0 released 2026-07-22, regular releases (0.17.0
  2026-02, 0.16.0 2025-09), 4.5M downloads
  ([crates.io](https://crates.io/crates/fuser)). **License**: MIT.
  Two flags: the maintainer states post-0.18 versions are "developed
  primarily by a coding agent" with human review, and external PRs are no
  longer accepted (issues only). API is synchronous callbacks — bridging to
  tokio means channel/block_on plumbing on our side.
- **Verdict**: the only maintained option and healthy enough, but a virtual
  mount is the highest-risk, lowest-necessity piece of Files. WebDAV
  (dav-server) already gives Finder/Explorer mounting on all three desktop
  OSes with zero kernel surface. Defer FUSE; when we do it, `fuser` is the
  crate.

---

## Shortlist (recommended stack)

| Concern | Crate | Version surveyed | License | Verdict |
|---|---|---|---|---|
| Chunking | `fastcdc` (v2020, tokio feature) | 4.0.1 | MIT | adopt |
| Hashing | `blake3` | 1.8.6 (1.8.5 locked) | CC0/Apache-2.0 | already in tree |
| CAS + verified streaming | `iroh-blobs` (`FsStore`, GC via `GcConfig`) | 0.103.0 | MIT/Apache-2.0 | adopt as beta; iroh 1.0 compatible |
| Watching (server + laptop local trees) | `notify` + debouncer, backed by our own scan/reconcile pass | 8.2.0 | CC0-1.0 | adopt; **never trust inotify on the NFS client for remote writes** |
| Server-wide watching (if inotify limits bite) | `nix::sys::fanotify` (`FAN_MARK_FILESYSTEM`) | nix latest | MIT | hold in reserve |
| Sync transport | iroh + iroh-blobs (existing stack) | — | — | build on it; no viable Syncthing crate |
| WebDAV bridge | `dav-server` (custom `DavFileSystem`, axum adapter) | 0.11.0 | Apache-2.0 | adopt |
| Storage locations | `object_store` behind our own placement trait | 0.14.1 | MIT/Apache-2.0 | adopt |
| Virtual mount (later) | `fuser` | 0.18.0 | MIT | defer |

**Cross-cutting notes**

1. Architecture consequence of the inotify finding: change detection is
   *authoritative on the storage server* (local inotify sees nfsd writes);
   clients get change feeds over our RPC (architect `#[subscribe]` streams),
   not from watching the mount. Laptop daemons watch only their local
   selective-sync copies.
2. Watchers are hints, scanners are truth: every backend drops events on
   overflow, so the version index must be reconstructible by a full scan.
3. Rename the crates.io `notify` dep (`package = "notify"`) to dodge the
   in-tree `features/task/notify` name collision.
4. fastcdc chunks + blake3 hashes + iroh-blobs `HashSeq` manifests give dedup,
   verified partial transfer, and GC-safe retention (tags per project
   version) from three already-compatible crates.

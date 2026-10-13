# Research: jj (Jujutsu) as the Files version engine

**Ticket:** #225 (part of #224, blocking #228)
**Method:** primary-source read of the jj repository at
`github.com/jj-vcs/jj`, commit `2e63a1e538171b051c48fae054ea71f82c6299cc`
(2026-08, jj-lib post-0.34). All file citations below are relative to that
checkout and that commit. No blog posts were used.

Files context (from #224): projects bound to on-disk File Roots; automatic
per-file version chains; no locking — concurrent saves survive as "Divergent
versions" merged later; multi-GB audio/video media; end-of-session
checkpoints; software projects still need real git.

---

## 1. Can jj's `Backend` trait sit on a media-scale CAS?

### The trait, as actually written

`lib/src/backend.rs` defines `pub trait Backend: Any + Send + Sync + Debug`
behind `#[async_trait]` (line 727–865). It is explicitly documented as "the
lowest-level trait for reading and writing commits, trees, files, etc."
(line 15–16). The full method surface:

| Method | Signature (condensed) | Notes |
|---|---|---|
| `name` | `&self -> &str` | written to `.jj/repo/store/type` at init |
| `commit_id_length` / `change_id_length` | `-> usize` | **hash length is backend-chosen** — a CAS can use its own (SimpleBackend uses 64-byte Blake2b-512) |
| `root_commit_id` / `root_change_id` / `empty_tree_id` | constants | |
| `concurrency` | `-> usize` | "A cloud-backed backend may want to set it to 100 or so" (line 752–761) — jj is designed for remote/async stores |
| `read_file` | `async (&RepoPath, &FileId) -> Pin<Box<dyn AsyncRead + Send>>` | **streaming read** — returns a reader, not bytes (line 764–768) |
| `write_file` | `async (&RepoPath, &mut dyn AsyncRead) -> FileId` | **streaming write** — takes a reader (line 772–776) |
| `read_symlink` / `write_symlink` | async, `String` target | |
| `read_copy` / `write_copy` / `get_related_copies` | async, `CopyHistory` | optional — "Backends that don't support copy tracking may return `BackendError::Unsupported`" (line 789–790); the Git backend does exactly that (`git_backend.rs:1097–1110`) |
| `read_tree` / `write_tree` | async, `Tree` = sorted `Vec<(RepoPathComponentBuf, TreeValue)>` (line 656–659) | trees are per-directory, git-style |
| `read_commit` / `write_commit` | async; `write_commit` takes an optional `SigningFn` and may rewrite the commit (authoritative server timestamps etc., line 821–838) | |
| `get_copy_records` | sync, returns `BoxStream<CopyRecord>`; streaming "by design to better support large backends which may have very large single-file histories" (line 848–851) | |
| `gc` | `(&dyn Index, keep_newer: SystemTime)` — everything reachable from the index is kept, plus anything newer than `keep_newer` to protect concurrent writers (line 859–864) | |

Object model (`backend.rs`): `Commit { parents, root_tree: Merge<TreeId>,
conflict_labels, change_id, description, author, committer, secure_sig }`
(line 191–223). Note `root_tree` is a **`Merge<TreeId>`**, i.e. a conflicted
commit stores multiple root trees natively (see §3). `TreeValue` is
`File { id, executable, copy_id } | Symlink | Tree | GitSubmodule`
(line 394–413). File content enters the model only as an opaque `FileId` —
the trait never asks a backend to diff, delta, or even report a file's size.

### Pluggability is real, not theoretical

`StoreFactories::add_backend` registers backends by name;
`default_backend_factories()` (`lib/src/default_backend_factories.rs:30–56`)
registers `SimpleBackend`, `GitBackend` (feature `git`), and a testing
`SecretBackend` this way, and the loaded repo picks its backend from the
`store/type` file. `Store` (`lib/src/store.rs:57–63`) wraps the backend with
LRU caches for commits (100) and trees (1000) only — **file contents are
never cached** (`store.rs:231–245` pass straight through), so a CAS backend
owns its own file-data caching policy. There is also a precedent for exactly
our use case: `concurrency()` and the streaming APIs exist because Google
runs jj against a cloud backend.

Two existing backends show the implementation cost:
- `SimpleBackend` (`lib/src/simple_backend.rs`, 581 lines total) is a
  complete local CAS backend: files are streamed in 16 KiB chunks through a
  Blake2b-512 hasher into a temp file, then renamed to
  `store/files/<hash>` (`simple_backend.rs:200–225`) — this is already a
  content-addressed file store, just without dedup-friendly chunking,
  compression, or gc (`gc` is `unimplemented!`, line 350).
- `GitBackend` (`git_backend.rs`, 2520 lines) shows the full-featured end,
  including extra-metadata sidecar tables for jj fields git can't store.

### Where multi-GB binaries hurt

1. **`read_file` implementations buffer today.** The trait returns a
   streaming reader, but both shipped backends materialize the whole blob
   first: SimpleBackend does `read_to_end` into a `Vec` then wraps it in a
   `Cursor` (`simple_backend.rs:183–198`); GitBackend does the same via
   `read_file_sync` → `Cursor::new(data)` (`git_backend.rs:1059–1065`).
   GitBackend's `write_file` also buffers: `contents.read_to_end(&mut bytes)`
   before writing the blob (`git_backend.rs:1068–1077`). **A custom backend
   is free to actually stream both directions — the trait supports it —**
   but the git backend specifically cannot be the media store (zlib blobs,
   full-buffer writes).
2. **Conflicted files are buffered whole.** When a path is in a conflicted
   state, snapshotting reads the entire file into a `Vec` to parse conflict
   markers (`local_working_copy.rs:1933–1951`,
   `conflicts::update_from_content`). Resolved files stream
   (`write_file_to_store`, `local_working_copy.rs:1993–2012`). So a
   multi-GB WAV that lands in a conflicted state would be read into RAM. For
   Files this is mitigable: binary conflicts can't carry markers anyway, and
   our Divergent-versions design keeps both sides rather than materializing
   marker soup (§3).
3. **No partial/chunked object model.** A `FileId` names one blob; there is
   no built-in chunking (FastCDC-style) or delta layer. Dedup across
   near-identical multi-GB project saves is entirely the backend's problem —
   which is fine for a CAS backend that chunks internally and treats
   `FileId` as the root of a chunk list, since jj never looks inside file
   contents.
4. **Checkout rewrites whole files.** `TreeState::write_file` streams
   store→disk via `copy_async_to_sync` into a newly created file
   (`local_working_copy.rs:2053–2080`); there is no reflink/hardlink/CoW
   path, so switching versions of a 10 GB video costs a 10 GB write.

**Verdict on Q1:** yes. The trait is small (~15 async methods), object-model
clean, explicitly designed for remote/high-latency stores, and file content
is opaque bytes end to end. The pain points are in the shipped backends and
in conflicted-file snapshotting, not in the trait contract.

---

## 2. Huge binaries + constant working-copy snapshotting

### How snapshotting actually works

jj auto-commits the working copy at the start of most commands
(`docs/working-copy.md:9–12`). The engine is
`TreeState::snapshot` (`local_working_copy.rs:1292+`) driving a rayon-
parallel directory walk (`FileSnapshotter::visit_directory`,
`local_working_copy.rs:1554–1593`).

The critical cost property: **cleanliness is decided by stat, not content.**
`FileState` records file type, mtime, and size
(`file_state()`, `local_working_copy.rs:934`); a tracked file is clean when
its current stat matches the recorded state and the mtime predates the state
file's own mtime (`get_updated_tree_value`,
`local_working_copy.rs:1828–1841`). A clean file is never opened. So an
unchanged 50 GB File Root costs one `readdir`+`stat` sweep per snapshot —
content is only read and re-hashed for files whose stat changed. This is the
same contract as git's index, and it is exactly the right shape for
end-of-session checkpoints.

### The knobs

- **`snapshot.max-new-file-size`** — default **1 MiB**
  (`cli/src/config/misc.toml:60`). Enforced only for files with no existing
  `FileState` — i.e. *new* files
  (`local_working_copy.rs:1692–1702`: the size check is guarded by
  `maybe_current_file_state.is_none()`); "Files that already exist in the
  working copy are not subject to this limit" (`docs/config.md:2105`).
  Oversized new files are reported as untracked with
  `UntrackedReason::FileTooLarge` rather than failing the snapshot, and the
  CLI suggests raising the limit (`cli/src/cli_util.rs:3294–3296`). `0`
  disables the limit (`cli_util.rs:1621–1622`). For Files we would set it to
  0 (or a Root-level policy) — it's an anti-footgun default, not a
  capability limit.
- **`snapshot.auto-track`** — a fileset choosing which new paths are
  auto-tracked (default `all()`, `cli/src/config/misc.toml:61`); combined
  with `.gitignore` handling in the walker
  (`local_working_copy.rs:1644–1686`). Files could use this to keep caches /
  peak files / freeze files out of versioning per Root.
- **Watchman fsmonitor** — optional feature; `TreeState` persists a
  `watchman_clock` in its proto state and asks watchman for the changed-file
  set since the last clock, turning the crawl into a visit of only changed
  paths (`local_working_copy.rs:1228–1257` `query_watchman`,
  `fsmonitor.rs`; settings `fsmonitor.backend = "watchman"`). For DAW-scale
  Roots this makes between-session snapshots near-O(changed files).

### Storage growth and gc

Every snapshot that finds changes writes new file blobs + new trees + a new
commit; the operation log (`docs/technical/concurrency.md:117–128`) keeps
old views alive. Reclamation is explicit: `jj util gc [--expire now]`
(`cli/src/commands/util/gc.rs:41–65`) first expires old operations, then
calls `Store::gc` → `Backend::gc(index, keep_newer)`. The contract
(`backend.rs:859–864`) is mark-and-sweep friendly: keep everything reachable
from the index, plus everything newer than `keep_newer` to protect
concurrent writers. GitBackend implements it by materializing no-gc refs for
all index heads and shelling out to `git gc`
(`git_backend.rs:1508–1536`); SimpleBackend doesn't implement it at all
(`simple_backend.rs:350`). **A custom CAS backend must implement gc itself**
— for media this is the difference between a Files server that grows forever
and one that expires abandoned intermediate saves. The `keep_newer` +
index-reachability contract maps directly onto CAS refcounting/mark-sweep.

One more growth vector: intermediate auto-snapshots. jj snapshots on every
command, so an 8-hour session with periodic saves of a 2 GB project file
produces many near-identical blobs. jj has no delta storage; only the
backend can dedup (chunking) — or Files simply runs jj snapshotting at
checkpoint granularity (Files' stated model is end-of-session checkpoints,
which sidesteps most of this; `jj util snapshot` exists for explicit
snapshots, `cli/src/commands/util/snapshot.rs`).

**Verdict on Q2:** stat-gated snapshotting scales to huge Roots; watchman
makes it incremental; `max-new-file-size` is a default we'd raise, not a
wall. The real obligations are (a) a gc implementation in our backend and
(b) dedup/chunking in the CAS because jj will happily write whole new blobs
per checkpoint.

---

## 3. Conflict / divergence model vs. Files' "Divergent versions"

jj has two distinct first-class mechanisms, and Files needs both:

### a) Concurrent operations → divergent changes (the "no locking" story)

jj "treats concurrent edits as a fact of life, not errors"
(`docs/technical/concurrency.md:18`). There is no repo-wide lock around
commits: operations and views are content-addressed objects, and the op-log
head is advertised via lock-free files-in-a-directory
(`concurrency.md:117–128`). When two writers race, the next reader finds two
op-log heads and **3-way-merges the view objects**, recording ref conflicts
as data ("moved from A to B or C", `concurrency.md:99–108`). If the same
change was rewritten on both sides, both commits stay visible and the change
is **divergent**: "a change that has more than one visible commit"
(`docs/glossary.md:137–141`). Nothing is discarded; the user (or Files UI)
reconciles later.

This is *precisely* Files' Divergent-versions semantics: two concurrent
saves of the same logical version both survive as siblings under one stable
identity (jj's `ChangeId`, which "follows the commit and is not updated when
the commit is rewritten", `backend.rs:52–56`), flagged for later merge.
`ChangeId` ≙ Files' per-version identity; visible commits of that change ≙
divergent versions; `jj evolog` (`cli/src/commands/evolog.rs`) already
renders the per-change evolution timeline.

### b) Conflicted trees as first-class objects

A conflicted merge is not an error state: the commit stores an odd-length
list of trees `A+(C-B)+(E-D)…` — `root_tree: Merge<TreeId>` in the commit
struct itself (`backend.rs:206`), documented in
`docs/technical/conflicts.md:15–42`. Contents are resolved lazily and
per-subtree ("we only need to merge parts of the tree that differ",
`conflicts.md:28–34`), and conflict expressions auto-simplify across rebases
(`Merge::flatten`/`simplify`, `conflicts.md:44–57`). The working copy
materializes text conflicts as markers and re-parses them on snapshot
(`docs/working-copy.md`, "Conflicts";
`conflicts::update_from_content` called from
`local_working_copy.rs:1955–1964`).

For Files this means a merged-later Divergent version is representable
*losslessly in the version graph*: merge the two divergent saves into a
child whose tree is a real `Merge` of both, resolve file-by-file whenever
the user gets to it. For binary media the "conflict" is simply the merge of
two `FileId`s — Files UI shows "version A / version B, pick or keep both",
which is a better fit than marker materialization (and avoids the §1
whole-file-buffering path by resolving via API rather than on-disk markers).

**Verdict on Q3:** the mapping is direct on both axes — lock-free concurrent
writes that both survive (op log + divergent changes), and unresolved merges
stored as data (`Merge<TreeId>` in the commit). This is the strongest single
argument for jj: we would otherwise re-implement exactly this machinery.

### One caveat

Divergence in a *single shared repo* is well-trodden; Files' concurrency is
often *multi-machine* (two editors saving to a synced Root). jj's op-log
merge handles any two ops on the same repo storage, including via
"concurrent modification … from different computers (via a distributed file
system)" which it aims to make merely "safe" (`concurrency.md:59–67`) — but
a Files *server* mediating pushes from two clients is our sync protocol to
build; jj gives us the merge semantics, not the transport.

---

## 4. Colocated-git mode for software projects

`docs/git-compatibility.md:100–124`: a colocated workspace keeps `.jj/` and
`.git/` side by side over the same object store; every jj command
auto-imports/exports git refs, so `git` tooling, IDEs, and build systems see
a perfectly normal git repo while jj adds the op log, auto-snapshotting, and
conflict model on top. `jj git init --colocate` / `jj git clone` create it
(colocation is the default; `--no-colocate` or `git.colocate = false` opts
out, `git-compatibility.md:123–124`), and `jj git colocation
enable|disable|status` converts existing workspaces in place
(`git-compatibility.md:173–196`).

Mechanics that matter for Files:
- The `GitBackend` stores jj commits *as* git commits, with a sidecar
  extra-metadata table for jj-only fields (`git_backend.rs`), so the repo
  pushes/pulls to GitHub unchanged.
- jj-only constructs degrade safely: conflicted trees are written as special
  git trees with `.jjconflict-base-N`/`.jjconflict-side-N` subtrees plus a
  `JJ-CONFLICT-README`, "This ensures that the parts are not GC'd"
  (`git_backend.rs:1540–1560` `write_tree_conflict`).
- Known costs: many-ref repos pay an import scan per command
  (`git-compatibility.md:142`), and `GitBackend::concurrency()` is 1
  (`git_backend.rs:1054–1056`).

**Verdict on Q4:** for a File Root that is a software project, Files can
simply run jj in colocated mode and get 100% real git (remotes, CI, IDEs)
while Files' own UI reads the same repo through jj-lib APIs. No custom
backend needed for this case — and crucially, this is the *same library and
data model* as the media case, so "Files version history" is one code path.

---

## 5. Deriving per-file version chains from jj tree history

jj has **no per-file version index** — like git, history is a commit DAG and
per-file history is derived. Available APIs, all in jj-lib:

- **Filtered revwalk:** the `files(pattern)` revset compiles to a predicate
  that walks candidate commits and asks "does this commit's tree differ from
  its parents under this matcher?"
  (`lib/src/default_index/revset_engine.rs:1376–1384`,
  `has_diff_from_parent`). Tree diffing short-circuits on equal subtree ids
  (`docs/technical/conflicts.md:28–34` describes the same lazy-subtree
  principle; `MergedTree::diff_stream` and friends,
  `lib/src/merged_tree.rs:283–330`), so per-commit cost is O(changed
  subtrees), not O(tree size).
- **Diff streams with copies:** `MergedTree::diff_stream_with_copies` /
  `diff_stream_with_copy_history` (`merged_tree.rs:311–330`) and
  `Backend::get_copy_records` (streaming, reverse-topological,
  `backend.rs:840–857`) let a chain follow renames. The backend even gets a
  `CopyHistory` object model (parents, salt for "logically new file at same
  path") that a custom backend can implement natively
  (`backend.rs:253–278`) — the Git backend doesn't
  (`git_backend.rs:1097–1110` returns `Unsupported`), but *ours could*,
  which upgrades per-file chains from heuristic to recorded fact.
- **Blame:** `lib/src/annotate.rs` implements per-line provenance;
  `jj file annotate` / `jj log <path>` / `jj evolog` are the CLI proofs that
  derived per-file history is a supported pattern.

**Cost assessment.** For Files' UI ("show me the version chain of
`Mix.wav`"), a naive walk is O(#commits on the Root) tree-diff probes —
fast for session-checkpoint cadence (hundreds to low thousands of commits
per Root), and each probe touches only the path's directory spine thanks to
subtree-id short-circuiting. If Roots accumulate 10⁵+ snapshots, we'd
maintain our own path→commits index as a derived cache (the `Store`'s
commit/tree caches at `store.rs:52–53` help the walk but don't replace an
index). Complexity: a few hundred lines over jj-lib (walk + diff-stream +
optional copy records), not a fork. jj's own `concurrency()`/streaming
design was built for "very large single-file histories" (`backend.rs:848`),
so the APIs won't fight us.

The genuinely nice extra: because `ChangeId` is stable across rewrites, a
Files "version" can be a change, and amendments to a checkpoint (metadata
edits, re-saves) stay one logical version with an evolog — matching Files'
"version chain" language better than raw git commits do.

---

## Recommendation

**(a) jj with a custom CAS backend** — jj-lib as the single version engine,
with a Files-owned media-scale content-addressed backend, and colocated-git
mode (stock `GitBackend`) for software Roots.

Why (a) over (b) hybrid: the split engine buys nothing — jj never inspects
file bytes, so one backend that chunks/dedups internally serves 4 KB text
and 40 GB video through the same 15-method trait, while a hybrid forces two
version graphs, two divergence models, and a router deciding which files are
"media". Why (a) over (c) CAS-native: question 3 is the decider — jj's
op-log concurrency merge + divergent changes + `Merge<TreeId>` conflicted
trees is a shipped, tested implementation of exactly Files'
Divergent-versions semantics, plus stat-gated snapshotting, watchman
incrementality, workspaces, and the gc contract. Rebuilding that CAS-native
is months of the hardest 20%; jj is Apache-2.0 Rust and `jj-lib` is a
published crate with backend pluggability as an existing extension point
(`StoreFactories::add_backend`).

Scope of the custom backend (the honest bill):
1. Streaming chunked blob store keyed by our `FileId` (SimpleBackend's
   16 KiB-streamed Blake2b writer is the 45-line template,
   `simple_backend.rs:200–225`; add FastCDC chunking + dedup).
2. `gc(index, keep_newer)` mark-and-sweep — mandatory, neither hard nor
   optional (SimpleBackend punts, `simple_backend.rs:350`).
3. Optional but valuable: native `CopyHistory` support for recorded per-file
   chains, and `concurrency() > 1` for a server-hosted store.
4. Policy: `snapshot.max-new-file-size = 0` (or per-Root), watchman on big
   Roots, checkpoint-cadence snapshots rather than per-command.

Known residual risks: conflicted-path snapshotting buffers whole files
(`local_working_copy.rs:1933–1951`) — keep binary divergence resolution in
the API/UI layer, not on-disk markers; checkout is full-rewrite (no
reflink); multi-client sync transport is ours to build on top of jj's merge
semantics; jj-lib is pre-1.0 (`predecessors` deprecation note at
`backend.rs:200–202` shows the API still moves) so pin and vendor-patch per
monorepo policy.

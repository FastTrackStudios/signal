# Repo split — migration plan

Splitting the FastTrackStudio monorepo (247 workspace members, 8590
commits, 4.85 GiB packed) into product repos. Successor to the August
2026 four-repo split that produced `architect`, `task` and `vendor`.

The allocation in this document is **derived from `cargo metadata`**, not
from directory names. `docs/split/paths.tsv` is the machine-readable
manifest and the single source of truth for the `filter-repo` invocations
below; regenerate it rather than hand-editing.

## Target topology

```
                    vendor ── architect ── task
                       │         │
                       └────┬────┘
                            │
                          daw          97 crates   the DAW platform + all shared substrate
                            │
                            ▼
                        session        30 crates   musical/production vocabulary + the Session app
                            │
                            ▼
                        signal        108 crates   the engine, the rigs, the plugins ("Signal Suite")
                            │
                            ▼
                   FastTrackStudio       7 crates   shell: site, docs-site, installer, the `fts` CLI

                        patchbay         5 crates   already split; in-tree copy is a dead duplicate
                        Ignition                    already split; repin its `daw` path dep to a tag
```

Cross-repo deps are **git deps pinned to a tag**, with local `[patch]`
overrides for co-development — the convention the August split
established. Never commit an override; the paths are machine-specific.

### Layer direction is the opposite of the runtime direction

At **runtime**, Session is the coordinator: it opens and syncs Signal and
Ignition over WebSocket, and depends on neither.

In **cargo** terms the arrow is reversed — Signal consumes Session's
crates:

| edge | why |
|---|---|
| `signal-sampler` → `keyflow-{annotate,orchestra}` | orchestral articulation vocabulary |
| `signal-orchestra` → `keyflow-orchestra`, `session-guide` | same, plus cue playback |
| `signal-synth` → `song` | song sections |
| `signal-guitar-ui` → `session-ui` | shared setlist widgets |

These are not accidents — the rigs genuinely speak the musical vocabulary
Session defines. So the `session` repo sits **below** `signal`: it holds
the vocabulary crates *and* the Session app, and Signal depends on the
former without ever touching the latter. The repo graph stays acyclic
because the coordination is over the wire, not over cargo.

Consequence for release order: **session tags before signal.**

## Verified allocation

Computed over non-optional, non-dev dependency edges. Result:

```
HARD upward edges:     0
OPTIONAL upward edges: 0
DEV upward edges:      1   (keyflow-proto dev-deps keyflow-text)
```

Full paths in `docs/split/paths.tsv`. The parts that differ from a
naive directory split:

**`features/reaper/` splits three ways.** It is not one block:

- `daw-*`, `reaper-*`, `fts-icons`, `fts-themer*` → **daw**
- `session-extension` → **session**
- `signal-extension`, `signal-reaper-controller` → **signal**

**`crates/keyflow/` splits 15/2.** Fifteen crates go to session as
intended. `keyflow-proto` and `keyflow-syntax` are forced into **daw**,
because `daw-reaper` hard-depends on `expression-editor-{daw,tools}`,
and `expression-editor-core` hard-depends on `keyflow-proto`. A wire
contract plus a syntax parser is legitimately foundation-layer, so this
is accepted rather than refactored around. (Breaking
`daw-reaper → expression-editor-*` would free all 17, but that is a
refactor, not a move — deferred.)

**`features/fx/` splits.** The three DSP cores `level-dsp`, `pitch-dsp`
and `tune-dsp` go to **daw** (the expression editor's pitch and level
tools need them); every other fx crate and all 19 plugin cdylibs go to
**signal**.

**`features/dynamic-template/` splits.** `color-palette` and
`music-catalog` → **daw** (`daw-actions` hard-depends on
`music-catalog`); the template engine, proto, extension and tests →
**session**.

**`apps/extensions/reaper-fts-extensions` → shell.** It is the aggregate
cdylib: it pulls `session`, `dynamic-template` and `keyflow-orchestra`
alongside the daw backend, so it belongs above all of them.

## Pre-flight code changes

Five changes must land in the monorepo *before* any history is rewritten,
so that each new repo's first commit already builds.

1. **Commit the pending `cargo fmt --all`.** The working tree holds a
   verified-mechanical reformat of 1000 files (byte-identical to
   `rustfmt(HEAD)`). Commit it as one `style:` commit, record its sha in
   `.git-blame-ignore-revs`, and add a `cargo fmt --check` gate to each
   new repo's CI — there is no fmt gate today, which is how the tree
   drifted. Doing this first means blame damage is one commit, and
   `filter-repo` carries the ignore-revs file into every repo.

2. **Drop `signal-fx` from the app's `session` feature.**
   `apps/fasttrackstudio/Cargo.toml` line ~142. It is the app-level form
   of the same inversion: the Session app must not link the engine's FX.
   Under the agreed architecture, Session gets FX from the Signal process
   over WS. Without this, the Session app cannot ship from the session
   repo.

3. **Fix the one dev-dep cycle.** `keyflow-proto` (→ daw) dev-depends on
   `keyflow-text` (→ session). Either inline the fixture the test needs
   or move that test to `keyflow-text`.

4. **Split `apps/fasttrackstudio` into two binaries.** Today one crate
   with `signal` / `session` / `full` / `tts` features. The `signal`
   feature set becomes the Signal app (signal repo, carrying `--engine`
   and the embedded web remote); the `session` feature set becomes the
   Session app (session repo). `full` has no successor — it was the mode
   that linked both, and that is exactly what the split forbids.

5. **Delete the in-tree copies of already-extracted repos.** An audit of
   every FastTrackStudios repo checked out alongside this one found
   exactly two duplicate clusters, nine crates in total:

   | cluster | in-tree paths | already lives in |
   |---|---|---|
   | patchbay (5) | `crates/patchbay/*`, `apps/patchbay/*` | `FastTrackStudios/patchbay` |
   | music-convention (4) | `libs/monarchy/{monarchy,monarchy-derive}`, `features/dynamic-template/{music-catalog,color-palette}` | `FastTrackStudios/music-convention` @ `v0.1.0` |

   Both were extracted on 2026-08-15 and left behind as copies. All nine
   are byte-identical to their extracted counterparts.

   - **patchbay** has *no consumers* in the tree — a pure deletion. But
     the monorepo copy had drifted **ahead**: it picked up the W10 OTLP
     telemetry work (`1b67f9c09`, 08-21) that the standalone repo never
     got. Port that first, or deleting loses it.
   - **music-convention** *is* consumed (`daw-actions` → `music-catalog`,
     `session`/`dynamic-template` → `monarchy`). Replace the four path
     deps with a tagged git dep. Both sides pin facet `0.50.0-rc.5`, so
     cargo unifies and the "one facet line" constraint holds.

   Doing this in the monorepo *before* the rewrite means it is verified
   once, rather than four times in four new repos — and stops `daw` from
   shipping a stale fork of monarchy forever.

   Both extracted repos were cut one day *before* the monorepo relicensed
   to GPL-3.0-or-later (`24b7114d1`, 08-16), so both still declare
   `MIT OR Apache-2.0` and neither carries a LICENSE file. See the open
   question below.

## Open question — the licence of the two extracted libraries

`patchbay` and `music-convention` missed the GPL relicense by one day.
`patchbay` has been brought to GPL-3.0-or-later to match. `music-convention`
has **not** been touched, because the call is less obvious: `monarchy`
(a hierarchy framework + derive macro), `music-catalog` and
`color-palette` are generic libraries with no FTS-specific content, and
they are the one part of the tree that would plausibly be published to
crates.io. GPL would make them unusable to most of that audience, and
relicensing is a one-way door.

A GPL work may depend on MIT/Apache code, so leaving music-convention
permissive costs nothing and keeps the option open. Confirm before
changing it — and say so if patchbay should be reverted to permissive
for the same reason.

## Split procedure

`git-filter-repo` is not installed; get it from nixpkgs rather than
`nix profile install` (see the den-fleet skill):

```bash
nix shell nixpkgs#git-filter-repo
```

For each repo, work from a **fresh mirror clone** — `filter-repo` refuses
to run on a repo with a dirty tree or existing remotes, and rewriting the
working checkout would break the running dev servers in sibling repos.

```bash
WORK=/run/media/Development/split-work
mkdir -p $WORK && cd $WORK
git clone --no-local /run/media/Development/FastTrackStudio daw-split
cd daw-split
git filter-repo $(awk -F'\t' '$1=="daw"{printf "--path %s ", $2}' \
                    /run/media/Development/FastTrackStudio/docs/split/paths.tsv) \
                --path LICENSE --path .git-blame-ignore-revs \
                --path-glob 'docs/**' --path-glob 'nix/**'
```

Shared infrastructure — `LICENSE`, `.git-blame-ignore-revs`, `flake.nix`,
`nix/`, `.cargo/`, `Justfile`, `.github/workflows/` — is copied into every
repo rather than assigned to one, then trimmed per repo afterwards.

Repeat for `session`, `signal` and the shell. `--no-local` matters: a
hardlinked clone would let the rewrite reach back into the source repo's
object store.

### Bootstrap order

Each repo needs its predecessor tagged before it can resolve:

1. `daw` — split, scaffold, green, tag `v0.1.0`
2. `session` — repin `daw` to the tag, green, tag `v0.1.0`
3. `signal` — repin `daw` + `session`, green, tag `v0.1.0`
4. `FastTrackStudio` — reduce in place to the shell (keeps its history),
   repin all three
5. `Ignition` — replace the machine-specific
   `path = "../FastTrackStudio/crates/daw/daw"` with the `daw` tag
6. `patchbay` — no change; only the monorepo copy is deleted

Between steps, use a local `[patch]` override so the loop is edit-and-test
rather than tag-and-push.

### Per-repo scaffolding

Each new repo needs, none of which `filter-repo` produces:

- a root `Cargo.toml` — `[workspace] members` from its slice of the
  manifest, and a `[workspace.dependencies]` table carrying its own path
  deps plus tagged git deps for the repos below it
- `flake.nix` + the `nix/` modules it actually uses. `daw` declares a
  **`rust-version` floor, not a pin**, so Ignition (1.95) and
  signal/session (1.94) can both consume it
- `.github/workflows/checks.yml` — fmt gate, clippy, nextest
- `CLAUDE.md` — the domain rules from the monorepo's, minus the parts
  that moved
- `LICENSE` — GPL-3.0-or-later in all of them. `libs/vendor/world`
  (BSD-3-Clause) and `libs/vendor/dioxus-test` (MIT OR Apache-2.0) keep
  their own and travel to `daw` with them

Docs travel with their domain: `crates/keyflow/docs` and the docs-site's
`kf` section go to session; `crates/signal/docs` to signal; the docs-site
itself stays in the shell and aggregates.

## Verification

A repo is not done until, from a clean clone with no `[patch]` overrides:

```bash
cargo check --workspace          # resolves against tagged deps only
cargo fmt --all --check
cargo clippy --workspace         # scoped with -p, never -D warnings
                                 # over the full workspace (see memory)
cargo nextest run --workspace
```

Plus per-repo smoke tests: `daw` → the REAPER integration suite;
`signal` → `just keys-test` and a rig that does not xrun; `session` →
the setlist round-trip.

## Cutover and risk

- The live rig's `signal-engine` unit is stopped for the duration; the
  binary becomes the Signal app and `just rig-install` is re-pointed
  afterwards. Nothing is deployed from a half-split tree.
- Sibling repos have live `dx serve` processes (`task/apps/web`,
  `Ignition`). The split happens in `$WORK` clones; the monorepo checkout
  stays intact and buildable until the new repos are green.
- The monorepo is not deleted. It becomes the shell repo, so its history
  and issue tracker survive in place.

# keyflow — extraction record

Splitting the notation domain out of `session` into
[FastTrackStudios/keyflow](https://github.com/FastTrackStudios/keyflow).
Successor to the four-repo split recorded in `PLAN.md`; same procedure,
one layer further down.

**Status: done.** `keyflow` is tagged `v0.1.0` and this repo consumes it.

## Why

Two reasons, and the second is the real one.

The chart language is not session-specific. `signal-sampler` and
`signal-orchestra` already reach for `keyflow-annotate` and
`keyflow-orchestra`; the task repo's Editor stack has
`editor-keyflow` / `editor-keyflow-lang`. Three products consume the
notation domain and only one of them is Session, so hosting it inside
the Session repo made every consumer take a dependency on the Session
app's release cadence.

And keyflow is about to grow a product of its own — the site at
`keyflow.fasttrackstudio.app` and a mobile app with a Keyflow-syntax iOS
keyboard extension. Those belong next to the language, not inside the
repo that ships the Session desktop app.

## Target topology

```
                    vendor ── architect ── task
                       │         │
                       └────┬────┘
                            │
                          daw
                            │
                        keyflow      18 crates   the chart language + Engraver
                            │
                        session      12 crates
                            │
                        signal
                            │
                   FastTrackStudio
```

## Allocation

Derived from `cargo metadata`, not from directory names.

**Moved (18):** all of `crates/keyflow/` (15 — `keyflow`, `-text`,
`-chordpro`, `-midi`, `-musicxml`, `-musx`, `-live`, `-sync`,
`-annotate`, `-orchestra`, `-daw-analysis`, `-ui`, `-lsp`, the CLI,
`tree-sitter-keyflow`) and all of `features/engraver/` (3 — `engraver`,
`engraver-proto`, `engraver-score`). Plus `docs/guides/keyflow/**` and
`docs/spec/score-engraving.md`, which travel with their domain.

**Stayed:** `chord-tool` and `chord-tool-daw`. `chord-tool` is a UI
panel and `chord-tool-daw` writes into a DAW project — both are
app-facing *consumers* of keyflow theory, not part of the notation
domain. They now take a git dep like everything else.

**Stayed in `daw`:** `keyflow-proto` and `keyflow-syntax`, unchanged
from the last split. `expression-editor-core`, which `daw-reaper`
hard-depends on, needs them, which makes a wire contract plus a syntax
parser foundation-layer.

## The one blocker, and the pre-flight change that removed it

Edges computed over non-optional and optional, normal and dev
dependencies:

```
DOWNWARD edges (fine, become git deps):  8
UPWARD edges:                            2   both from keyflow-ui
```

`keyflow-ui`'s `desktop-panels` / `wasm-panels` features depended on
`session` and `session-ui`, and `default = ["web"]` turned
`desktop-panels` on — so the upward edge was in every default build.

It was confined to `src/panels/{chart_view,preview_panel}.rs`, ~1300
lines reaching for `session_ui::{Session, ACTIVE_INDICES,
ACTIVE_PLAYBACK_*}` plus the dock. Nothing in the workspace consumed
`keyflow_ui::panels` — they were a leaf.

They were deleted (`5639ed3`). What remains is what the crate is for:
the renderer, layouts, signals, and the `ChartGraphics` mount. The
features are renamed `desktop-graphics` / `wasm-graphics`, and the crate
dropped `session`, `session-ui`, `daw-proto`, `dock-dioxus`, `dock-proto`
and `serde_json` entirely.

**The rule this encodes:** panels that wire a chart to *app* state belong
in the app, not below it. When the session app wants a chart panel back,
it writes one against `keyflow_ui::ChartGraphics` on its own side of the
boundary.

## Procedure

```bash
WORK=/run/media/Development/split-work-keyflow
git clone --no-local /run/media/Development/fts/session $WORK/keyflow-split
cd $WORK/keyflow-split
nix shell nixpkgs#git-filter-repo --command git filter-repo \
  --path crates/keyflow --path features/engraver \
  --path docs/guides/keyflow --path docs/spec/score-engraving.md \
  --path LICENSE --path .git-blame-ignore-revs --path .gitignore \
  --path .envrc --path flake.nix --path flake.lock \
  --path-glob 'nix/**' --path-glob '.cargo/**' \
  --path .config/nextest.toml --path Justfile --path .github/workflows
```

1507 commits in, 719 out — the real history of the domain, not a move
commit. `--no-local` matters: a hardlinked clone would let the rewrite
reach back into this repo's object store.

Then scaffolded by hand, since `filter-repo` produces no workspace: root
`Cargo.toml` (members + a dependency table with this repo's path deps
and tagged git deps below it), trimmed nix modules, a rewritten Justfile,
README and CLAUDE.md.

### What the scaffold had to fix

- **Profile overrides that match nothing are an error.** The inherited
  `[profile.dev.package.<name>]` entries were all audio-DSP crates
  (symphonia, rustfft, rubato, …) that do not resolve into a notation
  graph. Dropped.
- **Unused `[patch]` entries warn on every build.** `styx-format`,
  `phon` and `phon-jit` do not resolve there (`vox` comes in with
  default features off). Only the blitz fork survives.
- **A test fixture that escaped its crate.** `keyflow-text`'s
  `build_my_life` test did `include_str!("../../../../examples/…")` — a
  path outside the crate, pointing at a file present in **no commit of
  this history**. `cargo check` never noticed, because the call sits
  inside `#[cfg(test)]`; `cargo nextest run --workspace` fails on it in
  *this* repo today. Restored inside the crate at `tests/fixtures/`,
  reconstructed from what the test's own assertions specify.

## Verification

From a clean clone of the new repo, no `[patch]` overrides:

```
cargo check --workspace     green
cargo fmt --all --check     green
cargo nextest run --workspace
    1308 tests: 1278 passed, 30 failed, 18 skipped
```

The 30 failures read reference corpora (`lord_of_the_fight`, the
orchestra corpus) that are not in the repo. They fail identically here —
verified at the same commit — so they are pre-existing, not split
damage. They are documented in the new repo's CLAUDE.md.

This repo then: 13 members, `cargo check --workspace` green against
`keyflow` `v0.1.0` with no local override.

## What is next in the keyflow repo

Not part of this extraction, recorded so the repo's shape makes sense:

1. **`apps/site`** — `keyflow.fasttrackstudio.app`. A Dioxus landing page
   that demos the editor live, with chart state encoded in the URL so a
   chart is shareable as a link with no account. `keyflow-ui`'s
   `wasm-graphics` feature is the renderer path; it survived the panel
   deletion precisely because of this.
2. **The embedded guide** — `docs/guides/keyflow/*.md` rendered in-app,
   using the task repo's vault/editor layer, so the tutorial is written
   in Keyflow and read in Keyflow.
3. **A FastTrackStudio account** spanning the apps, at which point
   charts become saveable and shareable rather than URL-only.
4. **The mobile app** — chart management plus a custom iOS keyboard
   extension for Keyflow syntax.

Note the repo-level cycle that (1)–(2) introduce: `task` already depends
on `keyflow` (via `editor-keyflow`), and the site would depend on
`task`'s editor. Cargo stays acyclic — the site is above, the language
below, and they are different packages — but a change spanning both
needs two bumps in sequence, exactly as the session↔task arrow already
does. Use a local `[patch]` while developing; only the release needs the
round trip.

# Sample Space — implementation handoff (#77)

Live working notes for the M3–M5 build-out. Spec: `sample-space.md`.
Update this as milestones land; it is the resume point after any context
loss.

## Status

| M | What | State |
|---|---|---|
| M1 | `signal-space` core (analyze/classify/project/knn/store/CLI) | **DONE** `ba1b45827` |
| — | Piece granularity (a multisampled piece = ONE node) | **DONE** `f743b5914` |
| M2 | `signal-space-proto` + `SpaceBackend` + app "Samples" view | **DONE** `aeb3ca50a` |
| M3 | Electronic Kit rig (`features/rigs/ekit`) | **DONE** — backend `5782d0847`, engine mount + pad grid `012a2a73b` |
| M4 | Acoustic piece subspaces (`signal_drums::piece_space`) | **DONE** `27b59d25a` — RPCs shipped, drums UI list still to render |
| M5 | NAM space (`signal_nam::space`) | **DONE** `b8c1e63d4` / `ae1a7d8ba` — prober + similar/partner; guitar UI still to render |

## Hard-won facts (do not re-derive)

- **Granularity rule (user-stated)**: acoustic multisample libraries map at
  PIECE granularity — a kit piece with all its RR/velocity/mic files is ONE
  node. Per-file nodes are only for electronic one-shot folders.
  `build::Granularity::{Sample, Piece}`; CLI `--pieces`.
- Feature vector: `DIM = 56` = 48 log bands (mean-centered dB, tilt/level
  invariant) + 8 shape dims × `SHAPE_WEIGHT 3.0`. `ANALYSIS_SR = 22_050`.
- Store: `<root>/Space/<name>.space/{space.json, features.bin}`.
  `Space::load/save`, `Space::space_dir`.
- Proto lives at `crates/signal/space/proto` = crate `signal-space-proto`,
  module `space::`, trait `SampleSpace` (+ generated `SampleSpaceClient`,
  `Service`, `StreamService`, `SampleSpaceStreamSource`).
- Engine mount: `apps/fasttrackstudio/src/engine_main.rs`, after the
  pack-library block. `SIGNAL_SPACE_ROOTS` (colon-separated) selects roots.
- App view: `apps/fasttrackstudio/src/space_view.rs`, reached via
  `RigKind::Space` ("Samples") in `rig_view.rs`.

## Integration state (IMPORTANT)

`5782d0847` (M3 core) is on branch **`worktree/sample-space-m3`**, NOT main:
another agent had uncommitted engraver work in this tree and origin/main had
moved ahead, so rebasing/merging would have clobbered them. Merge the branch
into main once the engraver work settles (`git merge worktree/sample-space-m3`
from a clean tree). M1/M2 are already on main.

## What is left (all UI / polish — every engine path works)

1. **Drums UI**: render `similar_pieces(slot_id)` in the swap panel, and a
   "build piece space" action calling `build_piece_space()`.
2. **Guitar UI**: render `signal_nam::space::{similar_to, partner_for}` in
   the drive-block palette; archetype grouping headers.
3. **NAM archetype labels** need a second pass — neighbours are right but
   the label is driven by a knee metric that reads input-stage headroom
   (a boosted 5150 lands in "clean-warm"). Don't front a UI with the
   labels until fixed; the similarity lists are solid.
4. Ekit kit persistence (`.signalpreset` export) — deliberately deferred.
5. Auditioning in `SpaceBackend` still shells out to `pw-play`; wants a
   real engine preview lane.

## Validation on file (reproduce with these)

- `signal-space/examples/space_cli build <root> [--pieces]` — Luke Holland
  16 075 wavs → 26 piece nodes in 0.3 s.
- `signal-ekit/examples/pad_probe <space>` — 11/16 pads fill, 11/11
  audible, morph steps 9.
- `signal-drums/examples/piece_space_probe` — MM2: 48 engines in 14 s; a
  10" tom's neighbours are the 12"/13" toms.
- `signal-nam/examples/nam_space_probe <root>` + `nam_query <root> <substr>`
  — 259 community models in 66 s; TwinVerb Norm → TwinVerb Vibrato 0.988,
  5150 Boosted → 6505+ 0.862 (successor circuit). Library cloned at
  `/run/media/AudioHaven/Signal/NAM_models` (pelennor2170/NAM_models).

## Traps already paid for (encoded as comments in the code)

- Lock order: NEVER hold `rig` and `state` at once — the meter pump takes
  state→rig and deadlocked the whole rig.
- `std::sync::Mutex` is not reentrant: `render_hit` holding the rig lock and
  calling `trigger` (which relocks) self-deadlocks.
- Loose-wav instruments render SILENCE without an explicit
  `preload_instrument` — see `signal-sampler/examples/loose_wav_level.rs`.
- The bank maps a MIDI channel to exactly ONE instrument and `note_on`
  ignores its id → added `note_on_instrument` / `note_off_instrument`.
- Piece nodes must resolve to their LOUDEST velocity layer; the middle file
  is usually a whisper layer.
- Any directory walk over the sample roots must be depth-checked BEFORE
  `read_dir` (1.9M files; one level too deep = minutes).

## Repo conventions that bit me

- `#[architect::rpc]` goes on a trait INSIDE a `pub mod`, plain (no
  `ops(...)` arg). `#[subscribe]` methods are declared SYNC returning the
  event type: `fn events(&self) -> SpaceEvent;`.
- A service backend needs `#[derive(Clone, HasDispatcher)]` +
  `#[dispatch(CurrentThreadDispatcher)]`, `impl Services` with `layers![…
  ::Service, …::StreamService]`, and `architect` with features
  `["vox", "rig"]` for `architect::rig::events_hub()`.
- Rig crates: `features/rigs/<name>/{Cargo.toml,src/…}` + `proto/` + `ui/`
  (synth splits `rig/` for the backend — the closest sibling to copy).
  Register in root `Cargo.toml` members AND `[workspace.dependencies]`.
  Mount in `engine_main.rs`: `.merge_router(x.router())` then
  `mount_core!(router, "x", x.clone())`; add a `RigKind` variant + view.
- UI is Blitz-safe: INLINE styles only, no external CSS, no asset!().
- Ops: never `pkill -f fasttrackstudio` (live worship rig). Scratch engines
  run with `SIGNAL_ENGINE_ADDR=127.0.0.1:4141` and are killed by exact PID.
  One cargo command at a time (target lock).

## M3 plan — Electronic Kit rig

New crates: `features/rigs/ekit` (backend, crate `signal-ekit`),
`features/rigs/ekit/proto` (`signal-ekit-proto`), `features/rigs/ekit/ui`
(`signal-ekit-ui`).

Model:
- `Pad { index, category, space, item_idx, path, locked, params_locked,
  gain_db, pan, pitch, attack_ms, release_ms, filter (one-knob), reverse,
  choke_group, gate }`. 16 pads (4x4) but grid size is config.
- Pads play through `SamplerRig` + `kit_tracks`-style per-pad daw tracks so
  the mixer/sends/FX come for free. Pad N ↔ note 36+N (GM-ish).
- Kit ops (Atlas/XO semantics): `new_kit` (fill unlocked pads from each
  pad's category), `randomize_pad` (same-class KNN jump), `step_similar`
  (pad prev/next through its similarity list), `morph_kit` (step ALL pads
  together), `load_item` (drop a space item on a pad → pad category
  follows the item's class).
- Depends on `signal-space` directly for space loading + KNN (the rig picks
  samples itself; the SampleSpace RPC is for the browser map).

Open question deferred: kits as `.signalpreset` — v1 keeps kit state in the
rig and serializes to styx in the rig config dir; preset export comes after
the play path is proven.

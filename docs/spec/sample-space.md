# Sample Space — vectorized similarity browsing for audio assets

Status: spec, 2026-07-28. References: Algonaut Atlas 2 (manual read in full),
XLN XO, AudioStellar (GPL-3 — clean-room ideas only), the community
"Rig Sculpt / Rig Scope" NAM analyzer.

## The idea

ONE shared primitive set — an **asset space**: analyze every asset into a
feature vector, project the vectors to a 2D map (proximity = similarity),
classify into categories, and serve KNN similarity queries. Reused for:

1. **Electronic Kit rig** (new): a 4x4 pad grid over a huge folder of one-shot
   samples — XO/Atlas-style browsing, per-pad category, kit generation,
   similar-sample stepping.
2. **Drum Kit rig** (existing, acoustic): per-piece-class subspaces — we have
   many Snares/Kicks/Toms as `.signalengine`s; browsing a replacement snare =
   browsing the snare subspace.
3. **Guitar NAM models**: Rig-Scope-style probe analysis (measured EQ shape,
   input→output distortion curve, gain), archetype clustering, similar +
   "partner" (complementary, for stereo pairs) lists.
4. Later: cab IRs, reverbs, Omnisphere-style synth patches, loops.

The Drum Kit rig and the Electronic Kit rig stay **separate rigs** — one
emulates a real kit, the other is a pad instrument — but both sit on the same
space primitives and the same per-pad/per-piece daw-track mixer (`kit_tracks`).

## What the references taught us

**Atlas 2** (full inventory in session notes): pads carry a *category*; "New
Kit" fills unlocked pads from each pad's category; per-pad Randomize =
same-category resample; pad Lock vs parameter Lock; Earth mode = grid-snap +
cluster-collapse LOD (representative sample at low zoom); trigger-on-hover
audition; maps are portable files; kits embed their audio; analysis is
resumable with incremental rebuild-on-change.

**XO**: similarity is *list*-based — per-slot similarity list with prev/next
stepping, **Kit Similarity** steps all slots at once (whole-kit morph),
Hot-Swap replaces a slot live while the beat plays (OK/Cancel transaction).
Filters (text, category, main-freq, length, "drumminess", favorites, folder)
re-scope both the map and every similarity list. Live FS monitor for rescans.

**AudioStellar** (GPL-3; we reimplement ideas, never port code): the proven
pipeline — mono ≤1.2 s @ 22 050 Hz, features = STFT + MFCC + spectral
centroid + chroma + RMS → PCA → t-SNE (cosine, perplexity 30) or UMAP
(n=15, min_dist 0.1) → normalized 2D coords; DBSCAN (minPts 5, auto-eps) for
cluster colors; k-d tree for KNN. Project = a JSON cache the app just renders;
v2 runs the whole pipeline natively in C++ — no Python sidecar needed.

**Rig Sculpt/Scope** (NAM): analysis is *probe-based* — run a DI/sweep through
the model and measure: EQ shape (band magnitudes), input→output level curve
(distortion onset, compression), output gain. Bake results into asset
metadata. Derived UX: archetype → sub-flavor browsing, "similar" vs "partner"
metrics, auto stereo-pair build (phase check + level match), EQ "synergize".

## What already exists in-tree (survey 2026-07-28)

- **Scanners**: `signal_browser::pack_registry` (packs/engines/presets +
  TagSet, rayon, header-only), `signal_drums::library::scan_engines`,
  `signal_nam::scanner` + `NamCatalog` (sha256-keyed, tag facets).
- **Descriptor DSP**: `spectral_ab.rs`/`rr_cycle.rs` — 48-band log spectrum +
  mean-centered cosine (a working 48-D embedding!); `trigger-dsp` (6 onset
  ODFs, multiband kick/snare/cymbal heuristics, spectral fingerprint, HPSS,
  transient shape); `level-dsp::classify` (ZCR/centroid/flux/RMS per 10 ms,
  alloc-free); `signal_sampler::loudness::integrated_lufs` (BS.1770);
  `tune-dsp` YIN pitch; `meter-dsp::SpectrumAnalyzer`. FFT: `realfft`/
  `rustfft` already workspace deps. No MFCC in-tree (small to add).
- **Vector store pattern**: `wiki-search/src/vector.rs` — LanceDB schema +
  `DocEmbedder` trait + reciprocal-rank fusion (vector + token). Heavy dep;
  see storage decision below.
- **Tag similarity**: `signal_proto::tagging` — `TagSet::weighted_overlap`,
  `BrowserIndex::query` scored retrieval. Fuses with vector scores.
- **Map UI prior art**: `view-knowledge-graph` (deterministic FR layout +
  SVG pan/zoom/hover/label-density renderer, Dioxus),
  `signal_ui::PanZoomCanvas` (generic pan/zoom wrapper, Blitz-safe),
  `fts_ui_audio::axis` (value↔pixel math), `MappingView` (zone rectangles),
  `signal-grid-ui` (pad/grid components), `star_rating.rs`.
- **NAM probe infra**: `neural-amp-modeler` (inference),
  `nam_calibrate.rs` (DI clip → LUFS → makeup gain — half the probe
  pipeline already).
- **Sequencing/hosting**: pads = `kit_tracks` per-pad daw tracks (mixer,
  sends, FX slots all inherited); the daw is the sequencer — we do NOT build
  Atlas/XO's internal step sequencer.

Missing: the embedder pipeline (feature extraction orchestration + mel
filterbank), 2D projection (PCA + t-SNE/UMAP), KNN index, the space cache
format, the map UI component, the Electronic Kit rig itself, the NAM prober.

## Architecture

### `signal-space` (new crate, `crates/signal/space`)

Headless, wasm-clean data model; analysis behind a `native` feature.

- **Item identity**: `SpaceItemId` — a raw file (`path#sha256-prefix`), a
  pack zone (`pack-path#zone-idx`), an engine (`engine-path`), or a NAM model
  (`sha256`). Assets inside `.signalpack`s must be addressable without
  extraction.
- **`Analyzer` trait** (pluggable per asset kind) → `Descriptor`:
  - `features: Vec<f32>` (the embedding, kind-specific dim)
  - derived scalars: duration, loudness LUFS, spectral centroid ("main
    freq"), pitch + confidence, percussiveness ("drumminess"), attack/decay
  - `class: String` (kick/snare/hat/… or NAM archetype)
- **One-shot analyzer** (drums/electronic): mono mixdown, trim ≤1.5 s,
  resample 22 050; features = 48-band log spectrum (floored dB,
  mean-centered) + 20 mel-band MFCC-lite + envelope shape (attack ms, decay
  ms, crest) + centroid/ZCR/flatness + LUFS + YIN pitch. Class via
  multiband heuristics first (kick <150 Hz etc. — `trigger-dsp` prior art),
  optional ONNX classifier later (`ort` already in-tree via keyflow-sync).
- **Engine analyzer** (acoustic pieces): render a representative hit per
  engine (center velocity, mixdown of default mics via the offline sampler)
  → one-shot analyzer on the render. Places whole `.signalengine`s on a map.
- **NAM analyzer**: probe the model (silence → gain; stepped-level DI/sine
  → input-output curve, distortion onset; broadband noise/sweep → measured
  EQ curve in log bands; reuse `nam_calibrate` loudness). Features = EQ
  bands + IO-curve samples + gain scalars. "Partner" metric = engineered
  complement (similar IO curve, deliberately offset EQ) — ships after
  "similar".
- **Projection**: PCA (own small impl) → 2D t-SNE or UMAP (`bhtsne` /
  `umap-rs`; decide by output quality on the real library; deterministic
  seed), coords normalized 0..1. Cluster colors: DBSCAN with auto-eps, or
  class colors when classes exist.
- **KNN**: `kiddo` k-d tree over the full-dim vectors (cosine via
  normalized dot); similarity lists computed live, re-scoped by active
  filters (XO rule: filters narrow the map AND the lists).
- **Store**: `Space/<name>.space` dir next to the library — `space.styx`
  (items, classes, coords, scalars, favorites) + `features.bin` (packed
  f32 matrix + dim header). No LanceDB dependency for v1 (libraries are
  ~10⁴–10⁵ items; brute-force + k-d tree is instant); the store is
  schema-compatible with a later LanceDB backend if we outgrow it.
  Incremental rescan by (path, size, mtime, hash) — Atlas-style resume +
  rebuild-on-change.
- **CLI**: `fts signal space build|audit|similar <root>` (pack_cli sibling).

### `space-proto` (`crates/signal/space/proto`)

`#[architect::rpc] trait SampleSpace`: `spaces()`, `map(space) ->
MapModel{items: [{id, x, y, class, name, scalars…}]}`, `similar(space, id,
k, filters)`, `classify(path)`, `favorite(id, bool)`, `build(root)` with
`#[subscribe] events()` progress stream (resumable long analysis). Mounted
in the engine router; auditioning goes through the owning rig (preview =
trigger the sample through a preview lane, XO hot-swap style).

### Map UI (`signal-ui` component, Blitz-safe)

`SpaceMapView`: absolutely-positioned dots inside `PanZoomCanvas` (SVG only
if dot-count allows; the knowledge-graph view proves SVG to ~2 500 nodes —
Earth-mode-style grid-bin LOD above that: one representative dot per bin at
low zoom, expand on zoom-in). Class colors, hover = audition
(trigger-on-hover toggle), click = select, filters panel (class, text,
length, main-freq, favorites), similarity side-list for the selection.
Inline styles only; component takes no props it can get from context.

### Electronic Kit rig (`features/rigs/ekit`)

Separate rig crate (backend + proto + ui), mirroring the synth rig layout,
mounted scoped like the others; implements `RigCore`.

- **4x4 pad grid** (16 pads; grid size a config, not a constant). Pad =
  `{category, sample: SpaceItemId, locked, params_locked, params}` —
  Atlas semantics: drop-a-sample re-assigns pad category; Lock keeps the
  sample through kit generation; param Lock keeps params through swaps.
- **Kit generation**: New Kit (fill unlocked pads from each pad's category),
  per-pad Randomize (same-class KNN jump), per-pad similar prev/next
  stepping, **Kit Morph** (step all pads' similarity lists together), all
  auditionable while the daw plays (hot-swap is just an engine swap on the
  pad's lane).
- **Playback**: percussion-mode `SampleEngine` per pad on a per-pad daw
  track via `kit_tracks` (mixer/sends/FX/multi-out inherited; choke groups
  via the existing kit dispatch). Per-pad params v1: gain, pan, pitch ±12,
  attack/release shape, LP/HP one-knob filter, reverse, choke group,
  one-shot vs gate, fixed velocity.
- **Kits are presets**: `.signalpreset` with note routing (pad N → note
  36+N GM-style), so kits load like any preset; kit save embeds nothing —
  it references the space (our packs already solve distribution).

### Acoustic kit piece browsing (drums rig)

- Build the engine-space over the drum library (engine analyzer above).
- `swap_piece` UI becomes a space browse scoped by `kind_from_slot` — the
  snare slot opens the snare subspace, similar-list stepping auditions
  alternatives in context (the kit keeps playing; swap = existing
  `do_swap_piece`).

### Guitar NAM browsing

- Build the NAM space from `NamCatalog` (analysis keyed by model sha256,
  baked into the catalog next to tags — survives file renames).
- Drive-block palette gains: archetype grouping (DBSCAN clusters over the
  probe vectors, labeled by dominant tags), similar list, later partner +
  auto-stereo (needs the duplex DI measure step).

## Milestones

1. **M1 — space core**: `signal-space` crate: one-shot analyzer, PCA+
   projection, KNN, class heuristics, `.space` store, incremental rescan,
   CLI build/audit. Prove on a real electronic-sample folder + audit output.
2. **M2 — proto + map UI**: `space-proto` service in the engine,
   `SpaceMapView` + filters + similarity list in signal-ui, dock panel.
3. **M3 — Electronic Kit rig**: pad grid on kit_tracks, categories, New
   Kit / Randomize / similar-stepping / Kit Morph, kit presets.
4. **M4 — acoustic subspaces**: engine analyzer, per-piece browse in the
   drums swap UI.
5. **M5 — NAM space**: prober, archetypes, similar list in guitar UI;
   partner/stereo after.
6. **Later**: ONNX classifier (ort), Earth-mode LOD polish, favorites sync,
   IR/reverb spaces, proxy-quality preview streaming via pack-library.

## Non-goals (v1)

- No internal step sequencer (the daw is the sequencer).
- No cloud maps (pack-library proxy streaming already covers distribution).
- No Python sidecar — everything native Rust.
- No LanceDB until scale demands it.

# Frame → Control-Surface Engine: Research & Roadmap

Status: proposal (2026-09-28)

Goal: build control surfaces that AI can design, fully mapped and visually consistent, plus
embedded plugin/app UIs. The target is to rival MainStage and Gig Performer, with a much
better design story. The first consumer is `signal` (`fts/signal`).

Reference clones (read-only) live in `/run/media/Development/research/`:
`open-pencil`, `grida` (TS editor + Figma io), `grida-nothing` (Grida's Rust engine),
`figma-rust`, `figma-rs`.

## Status (2026-09-29)

**Built:**

| Area | What exists |
|---|---|
| Document | `document-proto`: typed `Scene` with hug sizing and `Node.control`; ops with inverses; `History` |
| Storage and text | `document-loro` (Loro replica, merge), `document-bundle` (`.frame` zip: styx manifest + `doc.loro` + content-addressed assets), `document-text` (lossless JSX-like markup + `REFERENCE.md`) |
| Import | `figma-import`: Figma → Scene. Scene rendering was proven identical to the old Figma path on all 26 parity scenarios; SVG export is byte-identical (130/130) |
| Surface | `surface-proto` (one `Curve`, `Catalog`, `Control`/`Binding`/`Slot`, `SurfaceEngine` service with `param_changes` + `telemetry` streams), `surface-memory` (mock engine), `surface-runtime` (gestures per the knob spec, slot driving, ballistics, coalescing, vox sync), `surface-widgets` (canonical controls, themes, `surface_for_catalog`) |
| AI | `design-lint` (11 rules, each with a fix), `design-session` (one tool registry; every call atomic and undoable), `frame-mcp` (stdio MCP; saves the markup file after every edit) |
| Apps | `frame` desktop player (hot reload, error banner, headless screenshot mode), `frame-cli` (`render_surface`, `export_markup`, parity tooling) |

**The design loop works end to end:**
```
frame-mcp --file stage.frame  ⇄  model (catalog → write/add_control → lint → fix)
frame stage.frame             ←  hot-reloads each edit; play it against the mock engine
```

**Next:**
- signal integration: publish a real `Catalog` from signal's rigs (`ParamTarget` on the wire), and a `SurfaceEngine` impl over vox.
- Design tokens as document variables with modes (theme switching without regenerating).
- Selector / XY / graph widgets (EQ, ADSR); modulation rings.
- `.fig` Kiwi import.
- Hosts: embedding in Blitz/Dioxus (signal-desktop), a baseview plugin window, native iOS.
- The editor UI (bind mode, MIDI learn) on architect-ui.
- Value-only render fast path and cached chrome, for large surfaces.

---

## 1. Where frame was (2026-09-28)

- **Figma fidelity is strong.** There are 26 parity scenarios and 23 pass. The 3 failures are
  background blur ×2 and single-pixel anchoring. It renders with vello and exports SVG.
- **The document model is a thin wrapper around Figma REST JSON.** Each `FrameNode` has a
  `raw: serde_json::Value`, and the renderer reads that raw JSON in about 70 places. There are
  only 3 edit ops: `InsertNode`, `RemoveNode`, `UpdateRawJson`. That is fine for parity work,
  but it is the main blocker for editing, bindings and AI authoring.
- **The editor is a skeleton.** It has a canvas, layer panel, property panel and toolbar.
- **Gaps found during research:**
  - text style runs (`characterStyleOverrides`/`styleOverrideTable`) are dropped
  - node `styles` references are not resolved into tokens
  - `vectorNetwork` is ignored
  - `.fig` is unzipped but never decoded
- **Version skew:** frame is on Dioxus 0.7.2 and facet 0.44, while signal and architect are on
  Dioxus 0.8 (git rev), facet 0.50 and vox 0.10. Moving onto architect (§4) fixes this.

## 2. What we learned from the reference projects

| Project | License | Take | Leave |
|---|---|---|---|
| **OpenPencil** (TS, CanvasKit) | MIT | `.fig` Kiwi schema + container + vector-network blob codecs; one **tool registry** feeding AI chat, MCP and CLI; **atomic, checkpointed AI transactions** (one undo entry per tool call, rollback on error); JSX authoring language + tested reference doc; `describe` + design lint rules; component-property model with id-based `propertyRefs`; variables with modes and aliases | wide all-optional node struct; materialized instance clones synced by a prop list; two text engines; relaying MCP over a WebSocket to the app; raw `eval` tool |
| **Grida / `nothing`** (Rust, Skia) | MIT OR Apache-2.0 | **`ChangeKind` invalidation** (None/Layout/Paint/Full + damage) with one dispatcher; **effect tree + selective GPU promotion** (cache expensive chrome, draw cheap shapes live); `FrameRenderStrategy`; `rframe`, a backend-free painter-ordered resolved frame; test taxonomy (reftest / golden / probe `.probe.json`); round-trippable LLM **text IR** (`n0-model/src/textir.rs`); "Rust owns all state" editor doctrine | Skia dependency (we're on vello); its lossy Figma import (instances flattened, bindings dropped) |
| **figma-rust** | MIT | style ID → token resolution; layout→CSS mutator passes (only if we ever emit HTML/CSS) | its flat schema types (less coverage than gridaco `figma-api`, which we already use) |
| **figma-rs** | none (can't use) | nothing | everything |

**None of these projects has parameters, value ranges or live bindings.** That layer is new
ground and the thing that sets frame apart.

## 3. What signal needs (summary)

- **Widgets:**
  - knob: unipolar/bipolar, modulation ring, detents
  - faders: V/H, console taper, plus edge fader and edge meter
  - meters: peak/RMS with ballistics, stereo, gain reduction
  - buttons: toggle, momentary, footswitch tile with 500 ms tap/hold
  - picker, segmented control, preset tree, XY pad, piano, pad grid, tuner
  - graph editors: EQ over spectrum, ADSR, filter curve, compressor trace, sequencer
  - node graph, pan/zoom canvas, lists and setlists, overlays, command palette
- **Consistency debt:** about 7 knob implementations, 1,275 hex literals, about 921 inline styles,
  3 curve systems that do the same job (`CcCurve`, `ResponseCurve`, `Taper`), and several
  1,900–2,300-line hand-laid panel files.
- **Parameter model:** `BlockParameter` stores a 0..1 value. `ParameterRange` holds
  `{min, max, taper, unit}`, and params are addressed as `ParamTarget { block_id, param_id }`.
  Macros are bindings with curves, plus MIDI learn and scenes vs snapshots.
  **The wire doesn't use `ParamTarget` yet.** Guitar calls `set_block_param(name, real_value)`;
  keys uses `(layer, module, id)`.
- **Runtime:**
  - every GUI is a remote over architect/vox (in-process, WS, SSE)
  - full-state events carry a `revision` counter
  - meters arrive at about 30 Hz and render at 60 fps with client-side decay; spectrum at 15 Hz
  - edits are coalesced to about 25 Hz, and the final value is always sent
  - targets: desktop (Blitz/vello), baseview plugin window, iOS, wasm
- **Stage use:** dark, high contrast, large touch targets, and layouts that scale from 720×480
  to 2560×1440.

## 4. Target architecture: an architect consumer repo

**Two principles:**
- **Figma is an import source, never the storage format.** Figma documents (`.fig` files, Figma
  clipboard data, REST JSON) are converted once into frame's own typed document. Figma JSON is
  not stored except as optional provenance (a Figma node id to frame node id map) so a re-import
  can update in place.
- **frame is built on architect** (github.com/FastTrackStudios/architect), the same stack signal
  uses.

### What architect gives us for free

| Need | architect piece |
|---|---|
| One source struct → wire, schema, storage | `#[derive(architect::Entity, Facet)]` |
| Live document with undo, offline use, merging, multi-device editing | `crdt` feature (Loro replicas synced over vox, fs persistence `snapshot.loro` + updates, compaction) |
| Editing API usable from the editor, CLI, AI and remote clients | `#[architect::rpc]` services + generated typed clients |
| Engine in-process (plugin, iOS) or remote (iPad over WS), same code | `architect::local` in-memory transport and `axum_ws` |
| Meter and telemetry streams | `PubSub::sliding(n).with_replay(1)`, `#[subscribe]`, `use_stream` |
| Widget library, component browser, **visual regression** | `architect-ui` + `architect-story` (headless Blitz snapshots, DSSIM diff, interaction fuzzing) |
| Specs linked to code (knob interaction contract, binding rules) | tracey, `features/<f>/spec/*.md`, enforced by `cargo xtask ci` |
| Wide-event observability | `architect-telemetry` |
| Version alignment with signal | architect already pins Dioxus 0.8 (git rev), facet 0.50, vox 0.10. Adopting it fixes frame's 0.7.2 / facet 0.44 mismatch |
| Signal integration | signal's engine already exposes `#[architect::rpc]` services, so surface bindings point straight at them |

### Repo layout

```
apps/frame/
  desktop/        frame-desktop   editor (Blitz/vello)
  web/            frame-web       editor + remote surfaces (wasm)
  cli/            frame-cli       import, render, lint, export-text, mcp
  tests/e2e/      frame-tests-e2e
crates/
  frame/          facade the apps (and signal) depend on; composes features
features/
  document/       the file format + model
    document-proto    typed IR entities: nodes, components+props+slots, variables+modes,
                      assets; typed ops (with inverses); DocumentService rpc
    document-loro     CRDT replica backend (live store, undo, collaboration, persistence)
    document-memory   plain in-memory backend (tests, headless render)
    document-text     round-trippable text IR (styx/JSX-like) for git diffs + LLMs
    spec/
  figma/          import only, one way into document ops
    figma-rest        REST JSON → document (ports today's frame-import)
    figma-kiwi        .fig + clipboard (Kiwi) → document (port of OpenPencil/Grida decoders)
    spec/
  render/
    render-proto      resolved, painter-ordered frame contract (à la Grida rframe);
                      ChangeKind {None, Value, Paint, Layout, Full} + damage
    render-vello      GPU painter (today's anyrender_renderer / vello path)
    render-svg        SVG export (today's svg/)
    tests/            Figma parity harness moves here (import → document → render vs Figma PNG)
    spec/
  surface/        the new ground: controls ⇄ engine
    surface-proto     ParamAddr, ParamCatalog, Binding{Param|Action|Stream|Collection|Macro},
                      one Curve type, MidiMapping, SurfaceService rpc
    surface-runtime   value store, gesture state machines, edit coalescing,
                      meter ballistics, transport adapters (vox local / ws)
    surface-ui        canonical primitives: knob, fader, meter, footswitch, picker, XY, …
                      (each one an architect-story with snapshots)
    spec/             knob.md, fader.md, binding.md, … (tracey rules)
  design-ai/      AI authoring
    design-ai-proto   tool registry as one rpc service (typed inputs, mutation kind)
    design-ai-lint    lint rules + `describe`
    design-ai-mcp     MCP adapter over the rpc service (in-process, no relay)
  editor/
    editor-ui         canvas, layers, properties, bind-mode, MIDI learn
xtask/            parity, svg_to_png, scaffold (existing commands move here)
```

### The file format

A `.frame` file is a bundle:

```
my-surface.frame/          (or zipped)
  manifest.styx            format version, doc id, title, required param catalogs
  doc.loro                 Loro snapshot: the entire typed document + its edit history
  assets/<blake3>.{png,ttf,…}   content-addressed images and fonts
```

- **Why Loro:** undo/redo and history come with it, concurrent edits merge, the same bytes sync
  live between editor, iPad and engine over vox, and architect's `crdt` already persists and
  compacts it.
- **Why a text IR as well:** binary CRDT files are poor for git diffs and for LLMs. `frame
  export-text` and `import-text` round-trip the document through `document-text`, and a tested
  law requires `parse(print(doc)) == doc`. Surfaces checked into signal's repo can live as text
  and be compiled into `.frame` bundles.
- **Validation, not trust:** a document references param catalogs by id and version. Loading it
  against a live engine checks every binding, so an unknown address is reported as a lint
  error rather than failing silently.

### Key design decisions

1. **Controls are components with typed properties and tagged slots, not special node types.**
   A knob is a component set: variants for size and style, a property schema of
   `value`/`bipolar`/`modAmount`/`label`, and slots marking which layer is the needle, arc or
   readout. Designers can draw any look, including in Figma, and the runtime drives the slots.
   One canonical library replaces signal's seven knobs.
2. **The binding is a first-class, validated edge:**
   ```
   Binding {
     control_prop: "value",
     source: Param(ParamAddr) | Action(ActionId, args) | Stream(StreamId, field)
           | Collection(CollectionId) | Macro(MacroId),
     transform: Curve (one curve type — unifies CcCurve / ResponseCurve / Taper),
     midi: Option<MidiMapping>,   // learnable on any control
   }
   ```
   Addresses resolve by id against a `ParamCatalog` that the engine publishes (facet-reflected).
3. **Tokens are variables with modes.** Modes are themes (stage-dark, high-contrast, daylight)
   and per-surface accents. Lint forbids raw hex values.
4. **Value-only render fast path.** A meter tick is `ChangeKind::Value` with a damage rect: no
   relayout and no re-resolve. Static chrome is cached and moving parts are drawn live
   (Grida's promotion model, on vello).
5. **AI tools and editor commands are the same `#[architect::rpc]` service.** Each call is one
   atomic Loro transaction and one undo step. The model reads and writes the text IR, checks
   itself with `describe` and `lint`, and MCP is a thin adapter over that service.

## 5. Phased roadmap

**Phase 0 — Move onto architect.** ✅ done on branch `architect-restructure` (2026-09-28)

- Layout: `apps/frame/cli`, `crates/frame`, `features/{document,figma,render}`, `xtask/scripts`.
- `frame-proto` became `features/figma/figma-model`. It is still the Figma-shaped model the
  renderer reads, and is transitional until `document-proto` replaces it in Phase 1.
- `frame-import` became `figma-rest`. `frame-ui` became `features/render/render`, with no
  Dioxus dependency. The binaries moved to `frame-cli`.
- The never-compiled editor skeleton and the unused Dioxus DOM renderer were dropped; they are
  still on `main`.
- `document` was scaffolded from architect's template (entity + service + memory backend, local
  vox round-trip test).
- Pinned to architect v0.9.4 (facet 0.50-rc.7, vox 0.10-rc.7), edition 2024, and a `flake.nix`
  with the Vulkan loader.
- **Gotcha:** consumers must repeat architect's `[patch.crates-io]` (vendored `phon`,
  `facet-core`, `vox-phon`). Without it, `architect::local` fails with "peer closed during
  handshake".
- Verified: `cargo test --workspace` is green, and render's 33 unit tests compile again (a stale
  test call was fixed). The parity suite matches `main` exactly: 23/26 pixel, 24/26 SVG strict,
  and identical per-scenario metrics.

Original Phase 0 plan:
- Restructure into the layout above. New features are scaffolded with `cargo xtask feature
  new`, not written by hand.
- Bump to architect's Dioxus/facet/vox pins.
- Move the parity harness under `features/render/tests` and keep it green. It is the safety
  net for everything after.

**Phase 1 — Own document model.**
- `document-proto` typed IR + ops, with a `document-memory` backend first, then `document-loro`.
- `figma-rest` emits document ops instead of wrapping raw JSON. The renderer reads only the
  typed IR (this removes about 70 raw-JSON reads).
- Fix the known Figma gaps as part of the port: text style runs, named style references
  becoming tokens, `vectorNetwork`, and `Unknown(String)` fallbacks.
- Components, properties, slots, variables with modes. `.frame` bundle load/save.
  `document-text` round-trip.

**Phase 2 — Surface runtime MVP.**
- `surface-proto` / `surface-runtime` / `surface-ui`, with tracey specs ported from signal's
  `docs/spec/signal/knob.md` and related specs.
- Primitives: knob, fader, meter, toggle/momentary, footswitch (500 ms tap/hold), picker. Each
  one is an architect-story with snapshot baselines.
- **Milestone:** signal's guitar perform grid and one amp panel rendered from a `.frame`
  surface, live over vox (in-process and WS).

**Phase 3 — AI + editor.**
- `design-ai` tool service + MCP + lint.
- Editor bind-mode (drag a param from the catalog onto a control), MIDI learn, preview against
  a mock engine.
- Templates: pedalboard, mixer strip, perform grid, macro bank ×8.
- **Milestone:** "design a keys perform surface for this rig" produces a fully bound surface
  that passes lint.

**Phase 4 — Reach.**
- `figma-kiwi` (`.fig` files and clipboard paste).
- Plugin window (baseview), native iOS, wasm remote.
- Graph widgets: EQ, ADSR, compressor trace, spectrum.

## 6. Work signal must do in parallel

- Put `ParamTarget`/`ParamAddr` on the wire (today guitar and keys each use their own
  addressing) and publish a `ParamCatalog` with range, taper, unit and default.
- Publish action and stream catalogs (`press_stack`, `tap_tempo`, meter/spectrum streams).
- Collapse `CcCurve` / `ResponseCurve` / `Taper` into one curve type shared with frame.

# Spectrasonics → Signal Domain Mapping

How Omnisphere / Trilian / Keyscape map onto Signal's
**Preset → Engine → Layer → Module → Block** hierarchy, where the gaps are,
and how to extend Signal so it can host Spectrasonics-class patches
**without their fixed limits** (8 Parts, 4 sub-layers per Part, 4 FX slots
per rack).

Reference data:
- [Architecture overview](./spectrasonics-synth-architecture.md)
- [Empirical parameter inventory across 42K patches](./spectrasonics-corpus-params.md)

## Domain hierarchy

```
Preset                    top-level scene; loads & routes Engines
  └─ Engine               ↔ Multi  (mixer + master FX, holds Layers)
       └─ Layer           ↔ Part   (MIDI routing, level/pan, holds Modules)
            └─ Module     reusable Block-graph bundle with exposed params
                 └─ Block leaf processor (Oscillator, Sampler, Filter, LFO,
                          Envelope, Arp, StepSeq, Tonewheel, FX, …)
```

**Key principle**: every processor is a `Block`. Oscillators and Samplers are
Blocks just like reverbs and filters are. A `Module` is a *reusable, parameter-
exposing bundle of Blocks* with their internal routing — equivalent to a
synth patch template or a Reaktor ensemble. The Spectrasonics `SubEngine` (the
canonical OSC + FILTER + AENV + LFOs + ModEnvs + WaveShaper + Dist + EQ +
ModMatrix + StepSeq path) becomes **a single reusable Module**.

This means:
- A Layer can hold **N Modules** (replaces Omnisphere's 4-sublayer cap).
- A Module can be the full-fat "Omnisphere SubEngine" preset, OR a stripped-
  down "Subtractive Voice", OR a custom user-built Block graph. All are
  equivalent at the Layer level.
- Modules are first-class citizens — saved separately, browsable in a
  library, drag-and-drop into any Layer.

## Concept map (Spectrasonics ↔ Signal)

| Spectrasonics                              | Signal              | Status |
|--------------------------------------------|---------------------|---|
| Multi (master mixer + master FX, 8 Parts)  | `Engine`            | ✅ existing — Engine's `Vec<LayerId>` is already unbounded; kills the 8-Part limit. |
| Part                                       | `Layer`             | ✅ existing — already has FxSends, macros, modulation. |
| Layer A / B / C / D (sub-layer in Part)    | `Module` × N in `Layer` | **gap**: Layer needs `Vec<Module>`. Unbounded. |
| SynthSubEngine (full synthesis path)       | a single canonical "Omnisphere SubEngine" `Module` (= a reusable Block graph) | new module template |
| `<OSC>`, `<MULTISAMPLE>`                   | `OscillatorBlock`, `SamplerBlock` | new Block types |
| `<FILTER>`                                 | `FilterBlock`       | new Block type |
| `<AENV>`, `<FENV>`, `<MODENV>`, `<MOD_ENV2_2>` | `EnvelopeBlock`     | new Block type (multi-segment, retrigger modes) |
| `<LFO>`                                    | `LfoBlock`          | new Block type |
| `<HARM>`                                   | `HarmonicBlock`     | new Block type (additive over the OSC) |
| `<DIST>`, `<WAVESHAPER>`                   | `DistortionBlock`, `WaveshaperBlock` | new Block types |
| `<EQ12>`, `<EQ2>`                          | `EqBlock`           | new Block type (parametric, configurable band count) |
| `<UNI>`                                    | `UnisonBlock`       | new Block type — discovered via corpus |
| `<DFS>`                                    | `DfsBlock`          | new Block type — Dynamic FM Synthesis (105K corpus instances; full RE pending) |
| `<TONEWHEEL>`                              | `TonewheelBlock`    | new Block type — Hammond/B3 emulation (43K corpus instances) |
| `<VOX>`, `<VOICE>`                         | `FormantBlock`      | new Block type — Innerspace / formant modeling |
| `<MOD_MATRIX>`                             | `ModMatrixBlock` (or routing primitive) | new — exposes any Block param as a target |
| `<SLICESEQSTEP>`                           | `StepSeqBlock`      | new Block type (slice-aware step sequencer) |
| `<ARP>` + `<ARPSEQ2>` + `<ARPFEELSEQ>`     | `ArpBlock`          | new Block type (groove/swing/feel patterns) |
| `<EFFMODULE Type="…">` (~80 effect types)  | one `Block` type per effect, OR a generic `Block` with parameterised effect-id | existing Block infra |
| `<Custom0..N>` macros                      | `macromod::MacroBank` | ✅ existing |
| Aux sends (4 per Part: `pNAuxSnd0..3`)     | `FxSend`            | ✅ existing |
| MIDI Learn (`*MidiLearnDeviceN/IDnumN/ChannelN`) | per-Block-param MIDI binding | new — alongside macro bindings |
| Live Mode / Stack profiles (`*.stack`)     | `Setlist` / `Song::Section` | ✅ existing |
| Master FX rack (`<MEffRack>`)              | `Engine`-level Block chain | ✅ existing concept |

## What "Module" actually is

A `Module` carries:

```rust
pub struct Module {
    pub id: ModuleId,
    pub name: String,
    pub kind: ModuleKind,                 // template identifier (e.g. "OmnisphereSubEngine")
    pub blocks: Vec<BlockInstance>,       // owned Block graph
    pub routing: BlockGraph,              // wires between blocks (audio + mod)
    pub mod_matrix: ModulationRouteSet,   // local mod matrix
    pub macro_bank: Option<MacroBank>,    // exposed parameter bank
    pub mix: ModuleMix,                   // level / pan / aux-sends within the Layer
    pub variants: Vec<ModuleSnapshot>,    // articulation/timbre variants
}
```

A `Module` is reusable in two ways:

1. **Template** — `kind` identifies a saved Block graph. New instances start
   from that template (e.g. drag "OmnisphereSubEngine" onto a Layer →
   instantiate the canonical 17-Block graph with default params).
2. **Snapshot** — once instantiated, you can save it as a new template, or
   reference it from multiple Layers (with copy-on-write or live-link
   semantics; design choice TBD).

This means Spectrasonics's "Layer A / B / C / D" inside a Part collapses to:
"a Layer holding 4 Module instances of kind `OmnisphereSubEngine`". And
"more than 4" comes for free.

## What "Block" actually is

A `Block` is the smallest processor unit. Blocks have:
- A typed input/output spec (audio in/out, mod-signal in/out, MIDI in/out)
- A parameter set (each parameter addressable via a stable path for
  modulation routing)
- A category (`Synthesis`, `Modulation`, `Effect`, `Control`, `Util`)

The corpus reveals the Block types Spectrasonics uses — listed in the
concept map above. Adding each is independent work; ordering by tier:

### Tier 1 — minimum subtractive voice (~10 Blocks)

`SamplerBlock`, `OscillatorBlock` (basic VA), `FilterBlock`, `EnvelopeBlock`,
`LfoBlock`, `ModMatrixBlock`, `DistortionBlock`, `EqBlock`,
`MidiInBlock`, `AmpOutBlock`.

This bundle, wired up as the canonical Module template, plays
~all Keyscape patches (sample + filter + amp envelope + 1 LFO).

### Tier 2 — Omnisphere parity (~10 more Blocks)

`HarmonicBlock`, `WaveshaperBlock`, `UnisonBlock`, `MultisegEnvBlock`
(richer than Tier 1's 4-stage), `StepSeqBlock`, `ArpBlock`,
plus more filter/dist algo variants, plus **wavetable mode** in
`OscillatorBlock` (consumes our `WavetableSpec` resource).

### Tier 3 — full feature parity (~5 more Blocks + DSP)

`DfsBlock` (Dynamic FM), `TonewheelBlock`, `FormantBlock`, `GranularBlock`
(if not folded into OscillatorBlock), `MogrifyBlock` /
`SpectrosynthBlock`. ~80 distinct FX block types from the EFFMODULE corpus.

## What this gives you

- **No fixed limits**. Layer can hold any number of Modules. Module can hold
  any number of Blocks. Engine can hold any number of Layers.
- **Composition over hardcoding**. The "Omnisphere SubEngine" is a *saved*
  Module template, not a hardcoded type — anyone can build their own
  Module from the same Block primitives.
- **Existing infrastructure reused**. Engine, Layer, FxSend, MacroBank,
  ModulationRouteSet, Block chain, Setlist — all already in `signal-proto`.
- **`.prt_omn` import is mechanical**. Each Spectrasonics XML tag maps to
  a Block instantiation with attribute → param mappings. The 38,745 .prt_omn
  patches become a regression test corpus.

## Scope of changes to existing code

Adding the Spectrasonics-class feature set to Signal:

1. **`signal-proto`**:
   - Add `Module` struct + `ModuleId` + `ModuleKind` (table-driven like
     existing `BlockType` / `ModuleType`).
   - Extend `Layer` with `pub modules: Vec<Module>`.
   - Add new `BlockType` variants for synth Blocks (Oscillator, Sampler,
     Filter, Envelope, Lfo, Harmonic, Waveshaper, Unison, StepSeq, Arp,
     Tonewheel, Dfs, Formant, …) + their parameter schemas.
   - Add `ModuleSnapshot` for variant/articulation switching at module
     level, mirroring existing `LayerSnapshot`.

2. **`signal-controller` / `signal-live`**:
   - Implement Block runtimes for each new Block type.
   - Implement Module runtime: instantiates Block graph, routes mod signals,
     exposes macros.
   - Module-graph audio thread integration.

3. **`signal-import`**:
   - `.prt_omn` / `.prt_key` / `.prt_trl` → Module instance (template
     `OmnisphereSubEngine` with attribute-mapped params).
   - `.mlt_*` → Engine (multi) with N Layers.

4. **`signal-ui`**:
   - Module browser (drag-drop templates onto Layers).
   - Block graph editor inside a Module (the inner routing canvas).

## Open questions

1. **Module composability**: can a Module contain another Module (nested
   ensembles), or only Blocks? Recommend nested for power; flag for cycle
   detection at save time.
2. **Block-param paths**: stable identifier scheme for mod matrix targets
   — `module/<id>/block/<id>/param/<name>` URL-style paths, or typed
   enum with extension points?
3. **Audio routing inside a Module**: explicit graph (every wire) vs
   implicit serial chain by Block order? Spectrasonics is implicit;
   modular synths are explicit. Recommend hybrid: serial by default,
   explicit graph available for power users.
4. **Per-parameter MIDI Learn**: separate concept from macro bindings, or
   unified under a single "control source" abstraction?
5. **Cross-Layer modulation**: can a Module in Layer A modulate a Block
   param in Layer B? Spectrasonics does this via the Multi-level mod
   matrix — Signal already has Engine-level `macromod::ModulationRouteSet`,
   should extend.

# Lighting control: OSS landscape vs. the industry leaders, and what FTS should build

**Status**: research / decision input. Nothing is implemented.
**Date**: 2026-08-23
**Question asked**: how do [ASLS Studio](https://github.com/ASLS-org/studio) and
[QLC+](https://github.com/mcallegari/qlcplus) compare to ETC Eos and grandMA3,
where are the gaps, and how feasible is a Rust/Dioxus/wgpu lighting product that
integrates with FastTrackStudio, runs on web + embedded hardware, and can
generate cues automatically?

---

## 0. Executive summary

1. **The two OSS projects are not small versions of Eos/MA — they are a
   different product category.** QLC+ and ASLS Studio are *playback engines with
   a patch and a cue pool*. Eos and grandMA3 are *programming instruments*: a
   command line, a referenced-preset graph, a tracking cue engine, and a
   multi-user session model. The gap is not features-in-a-list, it is three
   specific architectural things (§4): **referenced palettes/presets**,
   **tracking cue semantics**, and **an attribute abstraction layer** that
   decouples the show from the physical fixture.
2. **The hard parts of a console are not the parts that look hard.** DMX output,
   Art-Net/sACN, and a 44 Hz tick are a weekend each. The multi-year costs are
   (a) the **fixture/attribute abstraction + fixture library operation**,
   (b) **visualizer fidelity**, and (c) **workflow speed** — the thousand small
   affordances that let an operator program a show faster than they can think.
3. **A Rust/wgpu/Dioxus console is very feasible**, and FTS starts with an
   unusual amount of the plumbing already built: architect RPC + PubSub +
   CRDT doc-sync (multi-user and web remote *for free*), a headless-engine +
   detachable-GUI discipline that is exactly the console/node split real rigs
   use, `crates/input` + `features/surfaces` for control surfaces, styx for
   show config, and a wgpu/Blitz render pipeline. The genuinely new work is a
   lighting domain crate, a DMX/E1.31 I/O layer, and a 3D visualizer.
4. **The differentiator is not "another console".** It is that FTS already holds
   the *musical* structure of the show — `Section { section_type, start_seconds,
   end_seconds }`, `measure_positions`, tempo map, detected chords, the setlist,
   the guide/count-in engine. No console on earth has that. **Auto-generated,
   beat-locked, section-aware cue stacks that follow a setlist** is a product
   nobody sells, and it is the thing a volunteer-operated worship rig actually
   needs. Chasing Eos/MA feature parity is a trap; chasing *"the lighting
   programmed itself from the song"* is the win.
5. **Licensing is favourable.** FTS is GPL-3.0-or-later. ASLS Studio is GPL-3
   (compatible, readable as reference, code importable). QLC+ is **Apache-2.0** —
   its 1,735 `.qxf` fixture definitions and its engine can be brought in-tree.
   The Open Fixture Library (which ASLS's fixture JSON format comes from) is
   MIT. GDTF/MVR are open specs. There is no legal blocker anywhere.

**Recommended posture**: build the *engine and the automation*, borrow the
*data* (fixture libraries), and target *ASLS-plus quality* on the visualizer —
not Depence. See the roadmap in §9.

---

## 1. The four systems at a glance

| | **ASLS Studio** | **QLC+** | **ETC Eos family** | **MA Lighting grandMA3** |
|---|---|---|---|---|
| What it is | Web-based DMX control + 3D visualizer | Cross-platform DMX control suite | Theatre/broadcast console platform | Arena/busking console platform |
| Licence | GPL-3.0 | Apache-2.0 | Proprietary (ETCnomad free ≤ 512ch) | Proprietary (onPC free, limited output) |
| Stack | Vue 3, Vite, Three.js r170, Electron 41 | C++/Qt (two UIs: QWidgets v4, QML v5) | Proprietary + Unity-based Augment3d | Proprietary + built-in MA 3D |
| Repo scale | ~869 files, 163★, active (May 2026) | ~5,372 files, 1,508★, very active (Aug 2026) | — | — |
| Fixture library | ~430 OFL-format JSON defs | **1,735 `.qxf`** + 1,013 gobo images | ETC library + GDTF basic import (v3.2.4) | **Native GDTF** + GDTF Share |
| Output path | **Browser cannot emit DMX** → its own WSC binary protocol → Node gateway → Art-Net/sACN/DMX/MIDI/MSC/MTC/OSC | 20 native plugins: Art-Net, E1.31, DMX USB, MIDI, OSC, OS2L, HID, GPIO, SPI, uDMX, OLA, Peperoni, Velleman, Enttec Wing… | Net3/sACN/Art-Net, RDM via Gadget/Response, MSC, timecode, OSC | MA-Net, sACN, Art-Net, RDM, timecode, OSC |
| Visualizer | Three.js: volumetric beams, fog + turbulence, gobo/colour-wheel emulation, in-viewport transforms | v4: 2D monitor. v5: Qt3D `mainview3d` (beta) | **Augment3d** — integrated, MVR import (v3.2), used for focus/pixel-map | **MA 3D** — integrated, GDTF geometry, gobo visualisation |
| Multi-user | No | No | Yes (multi-console, partitions, tracking backup) | Yes (MA-Net sessions, user profiles, worlds) |
| Tracking cues | No (absolute scenes) | No (absolute scenes) | **Yes** (tracking, block/assert, cue-only, Mark) | **Yes** (tracking sequences, tracking-shield) |
| Referenced presets | No | Palettes in v5 (qmlui) | **Yes** (IFCB palettes + presets, everywhere) | **Yes** (preset pools are the core idiom) |
| Effects engine | 3 waveforms (sine/tri/square) + freq/min/max/phase/direction spread | EFX (pan/tilt math patterns) + RGB Matrix (42 JS `rgbscripts`) | Effects: step / absolute / relative / focus, patterns, per-channel | **Phasers** — per-attribute step lists with phase, width, speed groups |
| Timeline | Chase sequencer on a bar grid, BPM-synced, quantised, parallel lanes per group | Show Manager (tracks + timeline), Sequence | Cue lists (up to 12+), discrete/part timing | Sequences + Timecode pools |
| Command line | No | No | **Yes** — the primary interface | **Yes** — the primary interface |

### 1.1 ASLS Studio, concretely

Its domain model is small, legible, and honestly quite good — 24 files in
`src/models/DMX/`:

```
show → { master, universePool, fixturePool, groupPool, outputPool, live }
universe (512ch, patch map, address map, DMX512Data getter)
fixture ← capabilityManager / channel
group → cuePool → cue { scene | effect }
chase → chase pool (timeline of group cues on a bar grid)
```

- `Show` holds BPM and a tap-tempo, persists to localStorage / `.asls` files,
  has static undo/redo.
- `Universe.DMX512Data` renders a `Uint8Array(512)` from the patch each tick —
  a straight, readable output model.
- `effect.model.js` (1,126 lines) is the interesting file: `FXChannel` with
  waveform, direction, min/max, frequency, phase, and `fixturePhaseStart/Stop`
  fan spread, plus an `FXPresets` table. This is a *parametric effect engine*,
  which is more than QLC+ v4 offers for non-pixel fixtures.
- The visualizer (`src/plugins/visualizer/`) is 10 files: `scene_manager`,
  `model_instancer`, `moving_head`, `animation_manager`, `controls`, a z-up
  OrbitControls patch, and `shaders/beam.{vertex,fragment}.glsl`. One
  `moving_head_lowpoly.glb` does the work for everything.
- **The critical limitation**: a browser cannot open a UDP socket, so output
  goes over **WSC** (their own binary "Web Show Control" protocol) to a Node
  gateway that translates to Art-Net/sACN/DMX/MIDI/MSC/MTC/OSC. Elegant, but it
  means a second process and a bespoke protocol between you and the rig.

### 1.2 QLC+, concretely

Fourteen years old, and the engine header list reads like a console spec:

```
doc, universe, inputoutputmap, mastertimer, grandmaster, genericfader,
fadechannel, function → { scene, chaser, chaserrunner, collection, efx,
rgbmatrix (+ rgbscript/rgbimage/rgbtext/rgbaudio), script, sequence, show
(+ track, showrunner), video, audio }, cue, cuestack, fixture, fixturegroup,
qlcfixturedef/mode/head/channel/capability/physical, qlcpalette,
channelmodifier, monitorproperties, keypadparser
```

Strengths: **hardware breadth** (20 I/O plugins), **fixture library scale**
(1,735 defs + gobo bitmaps + 42 input profiles), a real function/timer engine
with HTP/LTP + grand master, Virtual Console (build your own operator surface),
web access for phone control, Raspberry Pi as a first-class target, 260 engine
tests, and coverage reporting.

Weaknesses: **two parallel UIs** — `ui/` (v4, QWidgets, what everyone ships)
and `qmlui/` (v5, QML, in beta for years, where the 3D view and palettes live).
No tracking, no referenced-preset graph, no command line, no multi-user, and a
Virtual Console that must be hand-built widget by widget rather than a
programmer model that scales with the rig.

### 1.3 What Eos and grandMA3 actually are

The thing to internalise is that **neither is primarily a playback engine**.
Playback is table stakes. What you are buying is:

**ETC Eos** (Apex/Gio/Element/Nomad) — the *theatre* answer:
- Tracking cue lists as the spine of the show; a cue stores only what *changes*,
  values track forward until something changes them. Block, assert, cue-only,
  Mark/AutoMark (dark moves), part cues, discrete per-channel timing, follow/hang.
- Palettes (Intensity, Focus, Colour, Beam) + Presets that are *referenced*, not
  copied: re-focus a Focus palette and every cue using it moves.
- Magic Sheets — designer-drawn interactive layouts.
- **Augment3d**: an integrated 3D visualizer (Unity-based) used not just for
  pre-vis but as a *working tool* — point-and-click focus in 3D, pixel mapping,
  MVR import since v3.2, basic GDTF import since v3.2.4.
- Colour science: Colour Path, gamut mapping across mixed-LED rigs.
- Multi-console: partitions, user IDs, tracking backup, sACN/Net3 output, RDM.

**MA Lighting grandMA3** — the *busking/arena* answer:
- Preset pools per attribute type, Sequences of cues driven by **Executors** on
  pages of physical faders/buttons — designed to be improvised live.
- **Phasers**: MA3's replacement for a classic effects engine. Every attribute
  can carry a step list with phase, width, speed groups, so "effect" is not a
  separate object type but a property of a value. This is the most powerful
  effect model in the industry and it is *why* MA looks like MA.
- **Recipes**: cue content described declaratively (this selection + this
  preset + this phaser) so content regenerates when the rig changes. This is the
  closest thing on the market to what §8 proposes.
- **Native GDTF** — MA co-authored GDTF; fixtures are the open format, not an
  internal one, and MVR carries the whole venue.
- Sessions over MA-Net with users, worlds, and filters; Lua plugins; timecode
  pools; hardware nodes for output.

Reference visualizers to calibrate against: **Depence²** (the current fidelity
ceiling — real-time volumetrics, media, water, physically-plausible), **Capture**,
**WYSIWYG**, **Vectorworks Spotlight** (design/MVR authoring).

---

## 2. Feature matrix by domain

Legend: ● full · ◐ partial · ○ absent

| Capability | ASLS | QLC+ | Eos | MA3 | Notes for FTS |
|---|---|---|---|---|---|
| **Patch & fixtures** | | | | | |
| Multi-universe patch | ● | ● | ● | ● | Trivial |
| Fixture modes / channel maps | ● | ● | ● | ● | Comes from the library format |
| Sub-fixtures / heads / pixels | ○ | ◐ (heads) | ● | ● (geometry tree) | **GDTF geometry tree is the right model** |
| Fixture library size | ~430 | 1,735 | huge | GDTF Share | Data problem, not code |
| GDTF / MVR import | ○ | ○ | ◐ (basic) | ● | Adopt GDTF natively from day 1 |
| RDM discovery | ○ | ◐ | ● | ● | Phase 3 |
| **Data model** | | | | | |
| Groups / selections | ● | ● | ● | ● | |
| Referenced palettes/presets | ○ | ◐ (v5) | ● | ● | **Gap #1 — architectural** |
| Attribute abstraction (colour as colour, not ch 5) | ◐ | ◐ | ● | ● | **Gap #2 — architectural** |
| Colour space / gamut mapping | ○ | ○ | ● | ● | Genuinely hard science |
| **Programming** | | | | | |
| Command line | ○ | ○ | ● | ● | Cheap to build, huge workflow win |
| Tracking cue engine | ○ | ○ | ● | ● | **Gap #3 — architectural** |
| Block / assert / cue-only | ○ | ○ | ● | ● | Falls out of tracking |
| Dark moves (Mark / MIB) | ○ | ○ | ● | ● | Falls out of tracking |
| Per-attribute discrete timing | ◐ | ◐ | ● | ● | |
| Effects / phasers | ◐ | ◐ | ● | ● | ASLS's model is a good starting point |
| Pixel mapping / matrix FX | ○ | ● (rgbscripts) | ● | ● | |
| Macros / scripting | ○ | ● (JS) | ● | ● (Lua) | |
| **Playback** | | | | | |
| Cue lists / sequences | ◐ | ● | ● | ● | |
| Executors on faders/pages | ○ | ◐ (VC) | ● | ● | Maps onto `features/surfaces` |
| Timeline / show mode | ● | ● | ◐ | ◐ | ASLS's bar-grid + BPM is *ahead* here |
| Timecode chase (LTC/MTC) | ◐ (via WSC) | ◐ (MIDI) | ● | ● | **FTS doesn't need it — see §8** |
| Grand master / blackout / park | ◐ | ● | ● | ● | |
| Priority / merge (HTP/LTP) | ◐ | ● | ● | ● | |
| **Visualisation** | | | | | |
| 3D viewport | ● | ◐ (v5) | ● | ● | |
| Volumetric beams + fog | ● | ○ | ● | ● | ASLS does this well already |
| Gobo / prism / colour wheel | ◐ | ○ | ● | ● | Projective texturing |
| Shadows / occlusion / bounce | ○ | ○ | ◐ | ◐ | Where cost explodes |
| Focus-in-3D as a workflow | ○ | ○ | ● | ● | High-value, moderate cost |
| **System** | | | | | |
| Multi-user sessions | ○ | ○ | ● | ● | **FTS has this already (CRDT+vox)** |
| Redundancy / tracking backup | ○ | ○ | ● | ● | Phase 3+; matters for live |
| Web / tablet remote | ● (is web) | ● (webaccess) | ◐ | ◐ | **FTS's native posture** |
| Embedded/SBC target | ◐ (Pi via Electron?) | ● (Pi) | ○ | ● (nodes) | `no_std` core rule applies |
| Show file portability | localStorage/.asls | XML `.qxw` | proprietary | proprietary | styx or MVR-adjacent |

---

## 3. Where the OSS projects actually fall short

Ranked by how much it hurts a real production:

1. **No referenced presets.** In Eos/MA, "warm amber" is an object. 300 cues
   reference it; the designer changes it once at 6pm and the whole show updates.
   In QLC+/ASLS, scenes hold literal channel values, so that change is 300
   edits. This single omission is the difference between "usable for a fixed
   installation" and "usable for a tour".
2. **No tracking.** Absolute-snapshot cues mean inserting a cue is safe but
   changing a look means touching every downstream cue. Tracking is *also* what
   gives you dark moves (Mark) — without it, moving heads visibly swing into
   position in front of the audience.
3. **No attribute abstraction.** If a show is written against channel numbers
   and per-fixture DMX layouts, swapping a Robe for a Martin in the touring
   pack re-programs the show. Eos/MA and GDTF express "Pan", "ColorAdd_R",
   "Zoom" as *attributes* with a geometry tree; the patch resolves them to
   channels at output time. **This is the single most important architectural
   decision in the whole project** — everything downstream (presets, effects,
   auto-generation, visualiser, fixture swaps) depends on it.
4. **No command line / no workflow speed.** `1 thru 20 at 50 enter` is not
   nostalgia; it is the fastest known interface for the job. Both OSS projects
   require mouse work per fixture. An operator who can't keep up with a
   rehearsal will not use the tool.
5. **No multi-user, no redundancy.** One laptop, one operator, no backup. For
   anything ticketed, that's disqualifying.
6. **Fixture library operations.** ETC and MA employ people to maintain fixture
   data and both back GDTF. Community libraries drift, contain errors, and lag
   new fixtures. This is an ongoing *operational* commitment, not a milestone.
7. **Visualizer as a toy vs. a tool.** ASLS's viewport is genuinely pretty, but
   Augment3d/MA 3D are used to *focus the rig* — click a spot on the 3D stage,
   the light points there. That inversion (visualizer drives the console, not
   the reverse) is what makes it earn its keep.
8. **Colour science.** Mixed rigs (RGBW + RGBAL + tungsten + CMY) matching a
   single "amber" across four fixture types is a hard colorimetry problem that
   Eos solves with Colour Path and neither OSS project attempts.

Notably, **the OSS projects are ahead in two places**: ASLS's BPM-synced,
quantised chase timeline is more music-aware than either console's cue stack,
and QLC+'s hardware/protocol plugin breadth beats what any single console will
talk to.

---

## 4. What's hard vs. what only looks hard

| Work | Real difficulty | Why |
|---|---|---|
| DMX512 / Art-Net / sACN output | **Easy** (days) | 512 bytes at ≤44 Hz. Crates exist. No audio-thread constraints — a dedicated 25 ms tick thread is plenty. |
| Universe merge, HTP/LTP, grand master, park | **Easy** (days) | Pure functions over a value stack. |
| Cue playback, fades, Bézier easing | **Easy** (1–2 weeks) | The DAW-side of this repo already does harder timing work. |
| Show file format | **Easy** | styx is already the config language here. |
| Fixture/attribute abstraction + GDTF | **Hard** (1–2 months, then forever) | GDTF is a big, quirky spec (geometry trees, DMX modes, channel functions, wheels, physical descriptions). Getting the *model* right matters more than the parser. |
| Effects / phaser engine | **Medium** (3–6 weeks) | Maths is easy; the *editing UX* is the work. |
| Tracking cue engine | **Medium** (3–6 weeks) | Subtle (block/assert/mark interactions) but well-documented and testable. |
| Command line parser + grammar | **Medium** (2–4 weeks) | QLC+ has `keypadparser.h` as prior art; the grammar is the design work. |
| Visualizer — beams, fog, gobos, bloom | **Medium** (1–3 months of GPU work) | Raymarched cones + projective gobo textures + HDR bloom in wgpu. ASLS-parity is reachable. |
| Visualizer — occlusion, bounce, media, Depence-grade | **Very hard** (years) | Do not attempt. |
| Multi-user, remote, persistence | **Already solved here** | architect RPC + PubSub + CRDT doc-sync + vox. This is FTS's structural edge. |
| Fixture library at scale | **Operational, unbounded** | Import QLC+ (Apache-2.0) and OFL (MIT); adopt GDTF; accept ongoing curation. |
| Workflow speed / muscle memory | **Hardest of all** | Not a feature. Earned over years with real operators. |

---

## 5. Rust ecosystem inventory (Aug 2026)

| Crate / project | ★ | Last push | Verdict |
|---|---|---|---|
| [`RustLight/sacn`](https://github.com/RustLight/sacn) | 61 | 2026-05 | **Use** — ANSI E1.31, tested against the protocol |
| [`Trangar/artnet_protocol`](https://github.com/Trangar/artnet_protocol) | 26 | 2026-02 | **Use** — 1:1 Art-Net implementation |
| [`rust_dmx`](https://crates.io/crates/rust_dmx) | — | — | Evaluate for USB widgets (Enttec etc.) |
| [`cpdt/gdtf-rs`](https://github.com/cpdt/gdtf-rs) | 1 | 2026-08 | Newest GDTF attempt; evaluate |
| [`BaukeWestendorp/rigger`](https://github.com/BaukeWestendorp/rigger) | 4 | 2026-05 | MVR + GDTF reader; also `mvr-gdtf` w/ MVR-xchange |
| [`michaelhugi/gdtf_parser`](https://github.com/michaelhugi/gdtf_parser) | 2 | 2023 | Stale |
| [`Firionus/opengdtf`](https://github.com/Firionus/opengdtf) | 6 | 2024 | Abandoned, but read its notes on GDTF's traps |
| [`maxjoehnk/Mizer`](https://github.com/maxjoehnk/Mizer) | 96 | 2026-08 | **Closest prior art** — node-based lighting software in Rust, Art-Net + sACN. Study its architecture. |
| [`BaukeWestendorp/radiant`](https://github.com/BaukeWestendorp/radiant) | 14 | 2026-08 | Rust console on GPUI (Zed's UI) — direct analogue of the Dioxus plan |
| [`BaukeWestendorp/zeevonk`](https://github.com/BaukeWestendorp/zeevonk) | 4 | 2026-05 | Headless lighting hub — same posture as `fasttrackstudio --engine` |
| [`matteolutz/demex`](https://github.com/matteolutz/demex) | 6 | 2026-05 | Command-line-driven DMX console in Rust — read its grammar |
| [`tiny-artnet`](https://github.com/D1plo1d/tiny-artnet) | 5 | 2022 | `no_std` Art-Net 4 — relevant to the embedded-node target |
| [`OpenLightingProject/open-fixture-library`](https://github.com/OpenLightingProject/open-fixture-library) | 255 | active | **MIT** — fixture data source, and the format ASLS consumes |

Takeaway: **no one has built the thing.** Mizer, radiant, zeevonk, and demex are
each one person's partial console; the protocol and GDTF layers exist but are
thin. There is room, and there is enough to stand on that nothing needs to start
from zero.

---

## 6. Licence position

| Source | Licence | Can FTS use it? |
|---|---|---|
| QLC+ engine + **1,735 `.qxf` fixtures** + gobo bitmaps | Apache-2.0 | **Yes** — importable into a GPL-3 tree, keeping notices. Biggest single win available. |
| Open Fixture Library | MIT | **Yes** — data + format |
| ASLS Studio | GPL-3.0 | **Yes** — same licence as FTS; readable as reference and importable |
| GDTF / MVR specs | Open standard | **Yes** — files come from GDTF Share (account required) |
| Eos / MA3 show files, libraries, UI | Proprietary | **No.** Interop by open formats only (MVR/GDTF, sACN, OSC, MSC, timecode). Clean-room only — same rule as the GPL reference macro in `features/fx/tune`. |

FTS's GPL-3 licence is a **one-way door** (per the root CLAUDE.md) and points the
right way here: everything worth borrowing is more permissive than GPL.

---

## 7. Proposed architecture in FTS terms

Name suggestion: **`lumen`** (domain) — parallel to `signal` (audio chain),
`daw`, `session`, `patchbay`.

```
crates/lumen/
  lumen-proto      # wire contract: #[architect::rpc] services, Facet types
  lumen            # facade — the ONLY public API surface (per signal's rule)
  lumen-core       # no_std + alloc: attributes, patch resolve, cue engine,
                   #   phaser maths, merge stack. No I/O, no threads, no alloc
                   #   on the tick path. Same rules as the DSP cores.
  lumen-io         # adapters: sACN, Art-Net, USB widget, OSC in/out, MSC,
                   #   RDM later. Native only; wasm gets none of it.
  lumen-fixtures   # GDTF + MVR + OFL + .qxf importers → one internal model
  lumen-viz        # wgpu 3D visualizer (own surface; see below)
  lumen-ui         # Dioxus/Blitz panels: patch, programmer, cue list, timeline
  lumen-live       # PubSub hubs, #[subscribe] streams, session/transport bridge
features/lumen-auto/   # cue generation from song structure  ← the differentiator
apps/                  # `fasttrackstudio --engine` gains a lighting engine mode
```

Mapping onto existing idiom:

- **Detachable GUI (STRICT)** already applies: the lighting core is headless,
  every GUI is a vox remote. That *is* the console/node/tablet split that Eos and
  MA sell as a hardware architecture. FTS gets it for free.
- **Output tick**: a dedicated OS thread (or `architect::platform::spawn` task)
  at 25 ms → resolve the merge stack → 512-byte frames → `lumen-io`. Never on
  the audio callback, never sharing a lock with it. The live-rig rule stands:
  a lighting stall must not be able to xrun audio.
- **Transport coupling**: `lumen-live` subscribes to the session playback /
  setlist streams in-process. **No LTC/MTC needed** when the console and the
  playback engine are the same binary — which is the whole point.
- **Surfaces**: `crates/input` + `features/surfaces/daw-csi` already speak MCU
  and MIDI; executors-on-faders is a new `zones.rs` mapping, not a new subsystem.
  The Kontrol S88 Light Guide work (raw USB bulk LEDs) proves the appetite for
  weird surface hardware.
- **Config**: rig files in styx, like `~/.config/signal/rig/*.styx`. Show data
  as a vault-adjacent document so CRDT sync applies.
- **Embedded**: `lumen-core` + `tiny-artnet`-style output on an SBC = a DMX node
  or a standalone "busking box" running the same cue engine as the desktop.

### 7.1 The one real technical unknown: 3D inside Blitz

Signal UI renders through `nice-plug-dioxus → Blitz (Vello + wgpu) → baseview`,
and the domain rule is **inline styles only, no external CSS**. Blitz has no
`<canvas>`, no WebGPU-surface element — so a 3D viewport is not a DOM node.
Three options, in order of preference:

1. **Sibling surface, shared device.** `lumen-viz` renders to its own wgpu
   surface/texture in the same window, composited under/over the Blitz layer.
   Requires a small amount of plumbing in the nice-plug/baseview window layer
   and a shared `wgpu::Device`. Cleanest long-term; the visualizer then works
   identically standalone, as a plugin, and docked.
2. **Render-to-texture, drawn as an image node.** Cheap to prototype, one frame
   of latency, and probably fine at 60 Hz for a pre-vis viewport.
3. **Web build uses WebGPU directly.** In the browser remote, the visualizer is
   a real canvas next to the Dioxus tree — arguably the *easiest* path, and it
   makes "runs on web" true first rather than last.

Recommendation: prototype (3) for the web remote and (2) natively, and treat (1)
as the eventual native answer. Note the known trap: `dioxus-native` has no
multi-window support (`add_window` skips context injection), so "visualizer in
its own OS window" is not a shortcut here.

---

## 8. The differentiator: cues that write themselves

This is the part no competitor can copy, because it requires owning the audio
and the musical structure. FTS already stores, in `crates/session/proto`:

```rust
Section { section_id, name, section_type, start_seconds, end_seconds,
          number, color, comment }
Song    { sections, comments, tempo, time_signature, measure_positions,
          detected_chords, chart_text, parsed_chart, count_in_seconds,
          advance_mode, color }
```

plus the setlist, the guide/count-in engine, keyflow's chord/lyric sync, and the
TTS cue layer. That means the following are *data transforms*, not new science:

- **Section → cue.** Intro / Verse / Chorus / Bridge / Turnaround each map to a
  look. A song's cue stack is generated from `sections`, with fade times derived
  from section boundaries and `color` seeding the palette.
- **Beat-locked phasers.** `measure_positions` + tempo map means chases are
  quantised to real bars, and they re-quantise when the tempo map changes. No
  console does this because no console knows the tempo map — they tap or chase
  timecode.
- **Energy curves.** Section type + arrangement density (which stems/parts are
  active, already known from `PartsManifest`) → intensity/saturation envelope.
  "Chorus 2 is bigger than chorus 1" for free.
- **Harmonic colour.** `detected_chords` + keyflow's key data → palette rules
  (relative-major warmth, minor coolness, a hit on the four chord). Gimmicky if
  overdone, powerful as a seed the operator then edits.
- **Setlist-wide looks.** A lighting "rig" is a preset, exactly as Signal rigs
  are (Drums / Keys / Guitar): one rig per venue, songs carry only *deltas*.
- **Guide integration.** The guide already speaks "Chorus in 2…" via TTS; the
  same schedule can pre-arm the next lighting cue, and the operator's surface
  can show it. A volunteer presses one button per song, or nothing at all.
- **Regeneration, not baking.** Store the *recipe* (selection + preset + phaser +
  section binding), resolve at playback — MA3 calls this Recipes; here it also
  survives a rig change and a tempo edit.

That is the pitch: **"you already programmed the lights when you built the
setlist."** Everything in §1–§4 is the price of admission for that feature to
exist in a tool people trust.

---

## 9. Roadmap and effort

Estimates assume one focused developer plus agent leverage, and *are not*
parity targets.

**Phase 0 — spike (2–3 weeks).** `lumen-core` value stack + merge; sACN and
Art-Net out via the existing crates; parse one GDTF and one `.qxf`; a wgpu
scene with one raymarched beam driven by live DMX; drive a real fixture. Goal:
kill the unknowns, especially §7.1.

**Phase 1 — MVP console (2–3 months).** Patch + fixture library import (QLC+ set
in bulk); attribute model; groups; **referenced presets from day one**; absolute
cue lists with fades; playback; Dioxus patch/programmer/cue panels; web remote;
styx show files. Deliberately *ahead of ASLS* on the data model, behind it on
polish.

**Phase 2 — the differentiator (2–4 months).** `features/lumen-auto`:
section→cue generation, beat-locked phasers, energy curves, setlist looks;
transport binding to the session engine; surface mapping (executors on faders);
timeline editor reusing the expression-editor/arrange machinery.

**Phase 3 — console credibility (4–8 months).** Tracking cue engine (block /
assert / cue-only / Mark); command line; effects/phaser editing UX; visualizer
fidelity pass (gobos, prisms, fog volumes, bloom, focus-in-3D); MVR import;
multi-user via the existing CRDT layer; embedded node build.

**Phase 4 — the long tail (ongoing).** RDM, colour-path/gamut mapping, redundancy
and tracking backup, fixture-library curation, hardware surfaces.

**Parity reality check**: the Eos/MA *feature surface* is on the order of 3–8
developer-years plus a permanent fixture-library function. That is not the goal
and should never be written down as one. "Better than QLC+/ASLS within a year,
and uniquely able to do something neither Eos nor MA can" is achievable.

---

## 10. Risks and open questions

1. **§7.1 (3D in Blitz) is the gating unknown.** Resolve it in Phase 0 before
   any UI work is committed to.
2. **Live-rig safety.** This machine's engine is Sunday-worship production gear.
   Lighting must be a separate thread/process boundary from audio, feature-gated,
   and never able to stall the audio graph. Test with the audio rig running.
3. **Scope gravity.** A console is a bottomless feature well. The §8 features are
   the reason to build; §1–4 are only the substrate. Ship the substrate thin.
4. **Fixture data quality.** Community defs contain errors. Needs a validation
   pass and a "trust level" per fixture, or the visualizer will lie.
5. **GDTF vs. internal model.** Adopt GDTF's *concepts* (geometry tree,
   attributes, channel functions) as the internal model, so import is a mapping
   and not a translation. Getting this wrong is the most expensive possible
   mistake.
6. **Open question — hardware.** Is the target a laptop, a rack node, or a box?
   The `no_std` core rule keeps all three open, but the I/O choice (USB widget
   vs. network node) should follow the actual rig.
7. **Open question — who operates it?** A volunteer pressing GO wants a very
   different surface than a designer programming a tour. FTS's "easy to use"
   requirement suggests the former is primary and the command line is Phase 3
   for a reason.

---

## Sources

- [ASLS-org/studio](https://github.com/ASLS-org/studio) · [WSC protocol](https://github.com/ASLS-org/WSC) · [live demo](https://demo.studio.asls.timekadel.com)
- [mcallegari/qlcplus](https://github.com/mcallegari/qlcplus) · [QLC+ docs](https://docs.qlcplus.org/)
- [GDTF Hub — MVR category](https://www.gdtf.eu/categories/mvr/) · [ETC releases Eos update with MVR support in Augment3d](https://www.gdtf.eu/blog/etc-releases-eos-update-with-mvr-support-in-augment3d/) · [GDTF/MVR improvements in grandMA3](https://www.gdtf.eu/blog/mvr-improvements-in-grandma3-2.4.2.2/index.html)
- [ETC community: GDTF + MVR feature requests](https://community.etcconnect.com/control_consoles/eos-family-consoles/i/feature-requests/gdtf-mvr)
- Rust: [Mizer](https://github.com/maxjoehnk/Mizer) · [radiant](https://github.com/BaukeWestendorp/radiant) · [zeevonk](https://github.com/BaukeWestendorp/zeevonk) · [demex](https://github.com/matteolutz/demex) · [rigger](https://github.com/BaukeWestendorp/rigger) · [gdtf-rs](https://github.com/cpdt/gdtf-rs) · [RustLight/sacn](https://github.com/RustLight/sacn) · [artnet_protocol](https://github.com/Trangar/artnet_protocol) · [rust_dmx](https://crates.io/crates/rust_dmx) · [tiny-artnet](https://github.com/D1plo1d/tiny-artnet)
- [Open Fixture Library](https://github.com/OpenLightingProject/open-fixture-library) (MIT)
- Repo evidence: `crates/session/proto/src/song.rs`, `features/song/src/model.rs`, `features/guide/`, `features/surfaces/daw-csi/`, root `CLAUDE.md` (signal domain rules, platform targets, licence).

# Form factors: one module, every shape

Status: design, 2026-09-29. Built so far: the `Embed` form and its host API
(`crates/frame-embed`, §4.9); the rest is design.

## The goal

The goal is to recreate Neural DSP-style amp-sim suites (pre-FX pedals, amp, cab, EQ, post-FX)
and our analog plugin faces (`examples/plugin-faces`) as frame documents. They should run on a
desktop, on an iPad and on an iPhone.

A **complete frame module** is one plugin or block, with one catalog of params, feeds and
actions. It can show itself in each of these shapes:

- fullscreen 16:9
- square 1:1
- a pedal
- a rack unit (1U by default; 2U, 3U and so on to match the real unit)
- a 500-series module (single or double wide)

Hosts then arrange modules into rigs: a pedalboard, a rack, a 500-series chassis, a Neural-style
suite window, and a stage perform view.

This doc covers:
- the physical and device standards the shapes come from (§1)
- what the reference material shows (§2)
- the recommended set of form factors (§3)
- the design: views, derivation, fitting, choosing a view, the library, boards and tools (§4)
- the Rust types and where they live (§5)
- a phased plan (§6)
- open decisions (§7)

---

## 1. Standards

Sizes are given as width × height of the face the player looks at. For pedals, the height is the
depth of the top face. **Design px** is frame's unit at `DESIGN_PX_PER_INCH = 60` (§4.1). Rows
marked "(2nd)" come from retailer or secondary sources only.

### 1.1 Studio formats

| Format | Physical face | Aspect (w:h) | Design px | Source |
|---|---|---|---|---|
| Rack unit pitch | 1U = 1.75 in / 44.45 mm | — | 105 | [Rack unit](https://en.wikipedia.org/wiki/Rack_unit) |
| Rack panel height | n·1.75 − 1/32 in: 1U 43.66 mm, 2U 88.11, 3U 132.56, 4U 177.0 | — | 1U 103.1, 2U 208.1, 3U 313.1, 4U 418.1 | same |
| Rack panel width | 19 in / 482.6 mm, including ears | 1U 11.05, 2U 5.48, 3U 3.64, 4U 2.73 | 1140 | [19-inch rack](https://en.wikipedia.org/wiki/19-inch_rack) |
| Rail opening | 17.75 in / 450.85 mm | — | 1065 | same |
| Ears | 0.625 in / 15.875 mm each side; holes 18.312 in / 465.1 mm apart; vertical pattern 0.5 / 0.625 / 0.625 in | — | 37.5 each | same |
| Half rack | 9.5 in / 241.3 mm | 1U 5.53, 2U 2.74 | 570 | [Rack unit](https://en.wikipedia.org/wiki/Rack_unit) |
| 500-series single | 1.5 × 5.25 in / 38.1 × 133.35 mm | 0.286 | 90 × 315 | [500-series](https://en.wikipedia.org/wiki/500-series_format), [VPR spec](https://medias.audiofanzine.com/files/api-vpr-500-spec-479213.pdf) |
| 500-series double / triple | 3.0 / 4.5 × 5.25 in | 0.571 / 0.857 | 180 / 270 × 315 | same |
| 500 chassis | 2, 6, 10 slots are common. Slate VMR shows up to 8 slots (A–H), 4–8 visible, and scrolls past that | — | — | [VMR docs](https://docs.slatedigital.com/VMR/Virtual_Mix_Rack.html) |
| Eurorack | 3U = 128.5 mm tall; 1 HP = 5.08 mm (0.2 in); rows of 84 or 104 HP | 84 HP row: 3.32 | HP = 12; 3U = 303.5 | [Doepfer A-100](https://doepfer.de/a100_man/a100m_e.htm) |
| Eurorack 1U tile | Intellijel 39.65 mm, Pulp Logic 43.2 mm | — | 93.7 / 102 | [Eurorack](https://en.wikipedia.org/wiki/Eurorack) |

Heights of classic units, which the faces should match (all 19 in wide):

| Unit | Height | Source |
|---|---|---|
| API 2500 | 1U | [apiaudio.com](https://apiaudio.com/product/2500-stereo-bus-compressor/) |
| SSL G-series bus comp | 1U (The Bus+ is 2U) | [SOS](https://www.soundonsound.com/reviews/ssl-xlogic-g-series-compressor) |
| UREI/UA 1176LN | 2U | (2nd) |
| Neve 33609 | 2U (not 3U) | [AMS Neve](https://www.ams-neve.com/outboard/limiter-compressors/33609-stereo-compressor/) |
| Teletronix LA-2A | 3U | (2nd, Sweetwater) |
| Pultec EQP-1A | 3U (the EQP-1A3 is 2U) | [Pulse Techniques](https://pulsetechniques.com/products/tube-equalizers/eqp-1a/) |
| Fairchild 670 | 14 in of panel, so 8U (one source says 6U) | [historyofrecording](https://www.historyofrecording.com/fairchild670.html) |

`plugin-faces` already follows this: `comp_fet.rs` draws the 1176 as 19 in × 2U (1120 × 204 px,
aspect 5.49). The tube faces (`opto_*`, `varimu`, `twin_valve`) are between 3.5:1 and 5.5:1 and
should snap to 2U or 3U.

### 1.2 Pedals and floor units

W × D in mm. Aspect is W/D. Design px are at 60 px/in (2.362 px/mm).

| Class (proposed) | Real reference | W × D mm | Aspect | Design px | Source |
|---|---|---|---|---|---|
| `Micro` | Hammond 1590A; Mooer Micro 42 × 93.5; TC Polytune mini 51 × 93 | 39 × 93 | 0.42 | 92 × 220 | [Hammond 1590](https://www.hammfg.com/electronics/small-case/diecast/1590), (2nd) |
| `Nano` | 1590B 61 × 112; EHX Nano 70 × 114 | 61 × 112 | 0.54 | 144 × 265 | Hammond; EHX (2nd) |
| **`Standard`** (default) | 1590N1 / "125B" 66 × 121 | 66 × 121 | 0.545 | 156 × 286 | Hammond; [stompboxlayout](https://stompboxlayout.com/tools/enclosure-size-comparison) (2nd) |
| `Compact` | Boss compact | 73 × 129 | 0.566 | 172 × 305 | Andertons (2nd) |
| `Wide` | 1590BB / BB2 | 94 × 120 | 0.78 | 222 × 283 | Hammond |
| `Medium` | Strymon small 102 × 114; Boss 200-series 101 × 138 | 102 × 114 | 0.89 | 241 × 269 | [Strymon](https://www.strymon.net/faq/what-are-the-dimensions-of-my-pedal/), [Boss DD-200](https://www.boss.info/us/products/dd-200/) |
| `Large` | 1590XX | 145 × 121 (usually mounted 121 wide × 145 deep) | 0.83 | 286 × 343 | Hammond |
| `BigBox` | Strymon TimeLine 171 × 130; Boss 500-series 170 × 138; 1590DD 188 × 120 | 171 × 130 | 1.32 | 404 × 307 | [Strymon](https://www.strymon.net/faq/what-are-the-timeline-pedal-dimensions/), [Boss DD-500](https://boss.info/products/dd-500/specifications) |
| `Treadle` | Cry Baby 102 × 254; Mission EP-1 99 × 251; Boss FV-500 110 × 289 | 102 × 254 | 0.40 | 241 × 600 | (2nd); [Mission](https://missionengineering.com/shop/ep-1/); [Boss](https://www.boss.info/us/products/fv-500h_500l/) |

Pedalboards (inches, W × D, rails): Pedaltrain Nano 14 × 5.5 (flat); Metro 16/20/24 × 8 (3
rails); Classic JR 18 × 12.5 (4 rails); Classic 1 22 × 12.5, Classic Pro 32 × 16 (4 rails);
Novo 18/24/32 × 14.5 (5 rails) ([pedaltrain.com](https://pedaltrain.com/products/pt-n24-sc)). Some
Classic figures are (2nd).

Floor controllers:
- Quad Cortex: 11 footswitches, 7 in screen, 290 × 195 mm.
- QC mini: 4 footswitches.
- Fractal FC-12: 2 × 6 footswitches over 512 mm, about 85 mm apart (derived).
- FC-6: about 94 mm apart.
- Boss FS-7: 75 mm apart, which is "tight" (anecdotal).

([neuraldsp.com](https://neuraldsp.com/quad-cortex), [fractalaudio.com](https://www.fractalaudio.com/fc-controllers/))

Amps (W × H):
- Marshall JCM800 2203 head: 740 × 315 mm, **2.35:1** ([marshall.com](https://www.marshall.com/us/en/product/jcm800-2203-vintage-reissue-head?pid=1007097)).
- Marshall 1960A 4x12: 770 × 755 mm, **1.02:1**.
- Fender '65 Twin combo: 26.5 in wide × 19.9–23.25 in tall, **1.15–1.33:1** (sources disagree).

### 1.3 Screens

| Device | Logical size (pt) | Aspect | pt per inch | Safe area (pt) | Source |
|---|---|---|---|---|---|
| Desktop 1920×1080 (50.5% of Steam) / 2560×1440 (21.9%) | same, or /1.25–2 when scaled | 16:9 | ~92–110 | — | [Steam survey](https://store.steampowered.com/hwsurvey/) |
| MacBook Pro 14 / Air 13 | 1512×982 / 1470×956 | 1.54 (16:10-ish) | ~127 | menu bar, notch | [9to5mac](https://9to5mac.com/2021/10/19/new-macbook-pro-screen-resolution-options/) |
| iPad 10th gen | 1180×820 | 1.439 | 132 | 24 top, 20 bottom | [Apple](https://support.apple.com/en-us/111840) |
| iPad Pro 11 (M4) | 1210×834 | 1.45 | 132 | same | [Apple](https://support.apple.com/en-us/119892) |
| iPad Pro 13 (M4) | 1376×1032 | 4:3 | 132 | same | [Apple](https://support.apple.com/en-us/119891) |
| iPad mini 6 | 1133×744 | 1.52 | 163 | same | useyourloaf |
| iPhone 15/16 | 852×393 | 19.5:9 (2.17) | 153 | portrait 59 top / 34 bottom; landscape 59 left/right, 21 bottom | [useyourloaf](https://useyourloaf.com/blog/iphone-16-screen-sizes/) |
| iPhone 16 Pro / Pro Max | 874×402 / 956×440 | 2.17 | 153 | 62 instead of 59 | same |
| iPhone SE | 667×375 | 16:9 | 163 | 20 status bar | [Apple](https://support.apple.com/en-gb/111866) |

Usable area once safe areas are removed:
- iPhone 15 landscape: 734 × 372 pt, **1.97:1** (signal's guitar rig forces this orientation).
- iPhone 15 portrait: 393 × 759 pt, **0.52**.

Hit targets:
- Apple: **44 × 44 pt** (visionOS: 60).
- Material: **48 × 48 dp** (can be smaller for pointer input).
- WCAG 2.2: **24 × 24 CSS px** for AA (2.5.8), 44 × 44 for AAA (2.5.5).

Sources: [Apple HIG](https://developer.apple.com/design/human-interface-guidelines/accessibility),
[Android](https://developer.android.com/guide/topics/ui/accessibility/apps),
[WCAG 2.5.8](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html).

Text read at a distance should have a cap height of 16–22 arcmin (ISO 9241-303). That works out
to about **5–6.5 mm at 1 m and 9–13 mm at 2 m**. At iPad density (132 pt/in), 2 m needs about
48–66 pt caps.

---

## 2. What the references show

### 2.1 Neural DSP Archetypes (`~/Downloads/Reference Images`)

**Window chrome.** The window is 16:9 (the in-app captures are 860 × 484). It has three parts:

- A **section nav** of six icons across the top: pre-FX, amp, cab, EQ, post-FX, plus one more.
- A **header strip** about 20% of the height:
  - Input knob with meter
  - Gate
  - Transpose
  - Input mode
  - preset bar (←/→, name, edit, save)
  - Doubler
  - Output knob with meter
- A **stage** holding the current section's hardware on a textured backdrop: wood for pedals, a
  stage for amps.

**Pre-FX (John Mayer X).** The pedals sit in one bottom-aligned row with even gaps:
- Justa Boost is **narrow**, aspect about 0.33 on screen (a Micro/Nano).
- Antelope Filter, Halfman OD, Tealbreaker and Millipede Delay are all the **same standard
  size**, aspect about 0.54 (a 1590N1/125B).

Every pedal has knobs on top, a label plate, then an LED and a footswitch in a fixed bottom
band. The other packs follow the same pattern: CWX, Nolly, THX and Nameless cutouts are all
about 0.55 (125B-like). Some pedals have a centred third knob, a mini toggle, or a 7-segment
display.

**Post-FX.**
- Dream Delay is a **wide/big box** (aspect about 0.92) with a display, two footswitches
  (Active, Tap) and mini toggles.
- Studio Verb is standard-sized.
- Asato's post-FX are two **tabletop units in a wooden stand**, rack-like faces at 3.4:1 and
  5.5:1. They are not pedals.

**Amp section.** There are three amps (a combo, a head on a 2×12, a head on a 2×12). Two
layers:
- A *rig overview* shows them all together on stage, with a translucent **master strip** laid
  over it: Gain, one knob per amp, Output, Room Send, Bypass.
- Tapping an amp opens its *close-up*: a crop of about 2.2:1 across the head's control panel.

This is exactly the "full view" versus "featured strip" split proposed in §4.3.

**Cab.** Several cabinet fronts (4x12, 2x12, 1x12) at their real relative sizes, about 1:1 each,
plus mic placement.

**EQ and compressor.** Tabletop "desktop units" with rubber feet, about 3.5–4.5:1 (JMX EQ and
comp, the THX 9-band graphic). These read as half-rack or 1U-ish faces, not pedals.

**Tabletop effect.** Gravity Tank is a small tabletop amp-style box, about 2.3:1 including its
grille, shown alone on the stage under the header.

**Wah.** A treadle, about 0.4 aspect.

**Takeaways:**
1. Real suites mix *physical* form factors: pedals of 2–3 size classes, heads, cabs and
   tabletop or rack units.
2. The *window* is a separate screen form: 16:9 chrome plus a stage.
3. Every section has a **featured-controls overlay** to use from a distance.

### 2.2 signal's rigs (`fts/signal`, read-only survey)

**Hosts:**
- Desktop defaults to 2560×1440, with a minimum of 720×480. At 1280 wide the guitar Control
  view is "legible but not usable".
- iPhone guitar is forced to landscape, with a 62 px left rail (Scenes / Control / Audio).
- iPhone keys is portrait, with tabs Play and Library, and a 15-key strip 88 px tall.
- Apple Watch uses a `WatchState` subset: stacks, FX, boost, tempo, tuner, song.
- Web demos mount the same components.

No rig UI has breakpoints. Responsiveness is done with flex weights and fixed px only.

**Guitar views** (`features/rigs/guitar/ui`):
- **Control** is a dense panel grid:
  - board rows for drive and amp/cab, 30 px each
  - a main row: Compressor (square), Pitch at 64 px, Amp EQ, Gate at 132 px
  - a time row: Modulation, Motion, Delay, Reverb
  - page two, in 280 px rows
  - rails 24 px and 86 px wide
  - every panel is a `ZoomPanel` that zooms to fill the screen
- **Routing**: a node graph with 76 px cells.
- **Presets** (tree) and **Tones** (browser).
- `ChainStrip` chips: "the compact edit surface for small screens".
- **PerformGrid**:
  - a **5 × 2 footswitch grid**: 5 physical switches × tap/hold with `HOLD_MS = 500`, and a
    3+4 chord opens the tuner
  - sizes **Full** (about ⅓ of the page), **Compact** (116 px) and **Hidden**
  - in Preset mode, a **pedal view** of stomp tiles (5 across)
- A **MacroBar** row with drop-down child knobs.
- A full-screen **Tuner** synced across remotes.
- A setlist sidebar with a section map.

**Keys views** (`features/rigs/keys/ui`):
- **Mixer**: 124 px `LayerStrip` channel strips grouped in `EngineStrip` cards.
- **Engine** and **Layer** zooms. Layer has a synth-style module editor with 260 × 72 graphs.
- A 7-card **macro band**.
- An **88-key keyboard strip**, 96 px tall, with split bands.
- A **PerformStrip**: one row of N stack buttons, at least 58 px tall.
- Routing and MIDI side panels, 360 px each.
- No organ drawbar component yet.

**Legacy `crates/signal/ui`:**
- a 4 × 2 Quad-Cortex-style scene grid
- compact "pills" versus detail cards (`module_view.rs`)
- `ParamCuration.featured_param_ids` (the "custom GUI" subset of 100+ raw params)
- a Kontakt-style sampler
- a 4 × 4 e-kit pad grid

**Takeaways:**
- signal already has several "compact" forms: the 116 px switch strip, chain chips, pills,
  featured params, the watch subset, and the macro bar.
- Its stage surfaces are *rig-level* (switch grids, macro rows, tuner, setlists), not
  module-level.
- The keys mixer is a row of narrow vertical strips, which is exactly the 500-series/VMR shape.

---

## 3. Recommended form factors

Two layers, kept separate:

- **Module form factors (views).** A shape one module's surface can take. They are listed in the
  module's document and chosen by hosts.
- **Board layouts (hosts).** Arrangements of *many* modules plus rig chrome. Each board asks its
  modules for a particular form: a rack asks for `Rack`, a pedalboard for `Pedal`, and so on.

### 3.1 Module form factors

"Complete" means the five forms the user asked for (★). The rest are recommended additions.

| Form | Kind | Canonical design size | Aspect | Variants | Fit | Touch rule |
|---|---|---|---|---|---|---|
| ★ **Full** | screen | 1600 × 900 | 16:9, reflows 4:3–21:9 | — | Reflow, letterbox outside the range | 44 pt at the smallest target host (iPad 1180 × 820, so scale 0.74 means hit boxes ≥ 60 design px); 24 px pointer |
| ★ **Square** | screen | 960 × 960 | 1:1 (0.8–1.25) | — | Reflow | same; iPad height 820 pt → scale 0.85, so hit boxes ≥ 52 px |
| ★ **Pedal** | physical | per size class (§1.2), Standard 156 × 286 | 0.40–1.32 | `Micro Nano Standard Compact Wide Medium Large BigBox Treadle`, plus an optional custom outline | Letterbox | knob hit box ≥ 44 at scale 1 (use `touch` padding); footswitch zone ≥ 60 × 60; 1–3 footswitches in the bottom 30% |
| ★ **Rack** | physical | 1140 × (n·105 − 2); 1U 1140 × 103 | 11.05 (1U), 5.48 (2U), 3.64 (3U), 2.73 (4U) | `units: 1..=8`, `width: Full \| Half` (570) | Letterbox, ZoomPan on phones | legible on desktop and iPad landscape (scale ≈ 1); on iPhone, zoom/pan or fall back to Strip |
| ★ **Lunchbox** (500-series) | physical | 90·slots × 315 | 0.286 (single), 0.571 (double) | `slots: 1..=3` | Letterbox | single-wide knobs ≤ 60 px, so at scale 1 hit boxes use the full 90 px width; a vertical stack of ≤ 6 knobs |
| **Head** | physical | 1200 × 510 (head), 900 × 780 (combo), 900 × 900 (cab), 1000 × 300 (tabletop) | 2.35, 1.15, 1.0, 3.3 | `Head Combo Cab Tabletop`, with custom w×h mm | Letterbox | the control panel is a strip about 20% of the height, so zoom into it when narrow (Neural's close-up) |
| **Strip** (phone portrait) | screen | 390 wide, height hugs | < 0.6, scrolls | — | Scroll (vertical) | 44 pt; one column; groups collapse; featured first |
| **Wide** (compact landscape) | screen | 800 × 400 | 2:1 (1.8–2.2) | — | Reflow | 44 pt at 734 × 372 (iPhone landscape): scale 0.92, so hit boxes ≥ 48 px |
| **Tile** | screen | 120·cols × 120·rows, grid of 1×1 … 4×2 | from grid | `cols, rows` | Letterbox | ≥ 58 pt on stage (signal's stack buttons); 1–4 controls |
| **Euro** (optional) | physical | 12·HP × 303.5 | from HP | `hp: 2..=104` | Letterbox | as Lunchbox |
| **Embed** | hosted | the host's box: `Strip` a strip's width (16–600) × the chain's height; `Mcp` 60–200 wide × the strip's FX area; `Tcp` 60–1200 × 22–80 | set by the host | `host: Strip \| Tcp \| Mcp` | Reflow by width tier (never letterbox) | the host's own (a mixer's pointer); 6 px grab zones on lines, ≥ 12 px handles |

Why these extras:
- **Head** covers amp heads, combos, cabs and tabletop boxes (Gravity Tank, Neural's EQ and
  comp units). In the references these are neither pedals nor racks.
- **Strip** is the phone-portrait form. signal has none today, and every module needs one.
  It derives automatically, so it is cheap.
- **Wide** is what a Full view becomes on an iPhone in landscape and in small plugin windows. At
  scale 0.46, a 16:9 layout's 44 pt hit boxes would need 96 design px.
- **Tile** is the stomp/perform tile, a Quad-Cortex pill, a macro cell or a chain chip. It is
  what a perform grid or `ChainStrip` shows per module.
- **Euro** is included for completeness with synth-module faces (keys rig). Low priority.
- **Embed** is a module drawn *inside another UI's strip*: a processor in our own mixer's
  channel strip (session's Tone rack: EQ, gate, compressor, saturator, de-esser, delay,
  reverb, one per row), an FX slot expanded in place, and REAPER's TCP and MCP inline FX UI.
  It is the one form whose size the **host** chooses, so it never letterboxes: the width
  picks a level-of-detail tier and the view reflows into the box (§4.9). Heights are a fact
  about the unit, not the width, so a unit sits on the same line across a mixer.

Deliberately not form factors:
- **Mixer channel strip** (as a module form). A mixer board shows each module's `Lunchbox`
  view as a column, which is how VMR works, and signal's 124 px `LayerStrip` is within 40% of
  it. What a *DAW* mixer strip carries inside it (a processor's curve under the fader's
  track) is an `Embed`, not a strip form.
- **Half-rack.** It is a `Rack` variant.
- **Tuner, setlist, footswitch map.** These are rig-level, not module-level.

### 3.2 Board layouts (hosts)

| Board | Arranges | Geometry | Notes |
|---|---|---|---|
| **Pedalboard** | `Pedal` views | Row mode: bottom-aligned, even gap, signal order (Neural). Board mode: a Pedaltrain-sized board, pedals free on rails with a 5 mm snap, cables drawn | Board presets: Nano, Metro 16/20/24, Classic JR/1/Pro, Novo 18/24/32 (§1.2) |
| **Rack** | `Rack` views | 19 in rails, 105 px per U, ears and screws drawn by the host (views may draw their own) | Half-rack views pair up in a tray; blank panels fill gaps |
| **Chassis** | `Lunchbox` views | N slots (2/6/8/10/16), 90 px pitch; a double-wide takes 2 slots | Horizontal scroll past the visible slots (VMR) |
| **Stage** | `Head` views | Heads on cabs on a floor, at relative physical scale | Neural's amp overview, with a master-strip overlay |
| **Suite** | sections of boards | A 16:9 window: section nav, a header chrome surface, and one board per section (pre-FX Pedalboard, amp Stage, cab Stage, EQ Rack or Stage, post-FX Pedalboard or Rack) | The Neural window. Chrome is itself a frame document |
| **Stomp** | footswitch jobs | `cols × rows` × tap/hold (guitar 5 × 2; QC 11; FC-12 2 × 6), switch pitch ≥ 80 pt | Replaces `PerformGrid`. Jobs bind actions, not module views |
| **Perform** | `Tile` views plus stage widgets | Grid of tiles, macro row, stomp strip, tuner overlay, setlist | Rig-level stage view; text sized for 1–2 m (§1.3) |
| **Keys** | module views plus keybed | Mixer of `Lunchbox` columns, macro band, 88-key strip (96 px), perform strip | signal keys |

---

## 4. Design

### 4.1 Units: physical forms in inches, screens in points

- A document's coordinates are **design px**. At host scale 1, one design px is one logical
  point (pt on Apple, dp on Android, CSS px on the web).
- Physical forms are defined in real units and converted at **`DESIGN_PX_PER_INCH = 60`**
  (2.362 px/mm). Both rack and 500-series standards are in inches, so the numbers come out
  round:
  - 1U = 105
  - 19 in = 1140
  - 500-series slot = 90 × 315
  - 1 HP = 12
- It also matches today's faces: the 1176 is 1120 wide, about 59 px/in.
- A real 20 mm knob comes out at 47 px. So a face drawn at life size meets 44 pt at scale 1
  when its knobs are life-sized. Scaling below 1 is what breaks touch, and §4.5 handles that.
- Screen forms (Full, Square, Wide, Strip, Tile) are defined directly in pt.

### 4.2 A view is a page

Today a document already has pages (`Scene.root → Page*`), and every host shows the first page
(`pages()[0]` in the desktop player, library preview, `render_surface`, `kit_sheet`). A **view**
is a page plus metadata:

- **One document, one catalog, many views.** The params, feeds and actions are the same for
  every view. Each view is its own scene tree, sized to its form, and binds a subset.
- **Two views on two devices stay in sync without extra work.** The engine is the source of
  truth for values, so the phone's pedal view and the desktop's rack view both follow
  `fet/input`. There is nothing to sync between views.
- **Stored alongside, not on nodes.** `Scene.views: Vec<View>` (serde-default empty). Each view
  points at its page. A document with no views has one implicit view: its first page, with the
  form inferred from its root frame's aspect (`FormFactor::infer`). The 1176 face at
  1120 × 204 reads as `Rack { units: 2 }`. So every existing `.frame` keeps working.
- **Markup:** view metadata goes on `<Page>` attributes, so the text IR round-trips it:
  ```xml
  <Page name="Rack 2U" view="rack" form="rack:2u" fit="letterbox">…</Page>
  <Page name="Pedal" view="pedal" form="pedal:standard" fit="letterbox" derived-from="rack">…</Page>
  <Page name="Phone" view="strip" form="strip" fit="scroll-y" derived-from="rack">…</Page>
  ```
- **Parts page (optional).** A non-view page named `Parts` holds components that several views
  instance, such as a knob with its skin or a VU card. It uses the Component/Instance machinery
  from roadmap Phase 1. Editing the component restyles every view. Instance overrides are the
  per-view overrides.

### 4.3 Inheritance and derivation

Views relate in two ways:

1. **Shared look through components** (above). This is the long-term mechanism: a pedal and a
   rack face share knob, lamp and panel-paint components but differ in layout.
2. **Derived views** (`ViewOrigin::Derived`). `derive_view(from, form)` builds a new view from a
   source view and the catalog. The result is **materialized**: ordinary nodes, editable like
   anything else. It records:
   - the recipe (form, what was picked, the template)
   - `links: node in the derived view → node in the source` for each copied control
   - `detached`: the nodes a person or agent has since edited by hand

   `refresh_view` re-runs the recipe and replaces only linked, non-detached nodes. So "edit the
   rack face's knob skin" flows into the pedal, while a hand-moved pedal knob stays put.

   Why materialized rather than generated at runtime:
   - It can be linted, previewed, diffed and hand-tuned.
   - Hosts stay dumb.
   - A runtime auto-derivation (§4.5 step 4) is only the fallback for documents that lack a
     view.

**What gets picked: priority, then budget.**

Each control in the source view is ranked:
1. A param `role`:
   - `Bypass` → footswitch
   - `Tap` → second footswitch
   - `Hero` → the big knob
   - `Output`, `Mix`
2. `ParamInfo::priority` (proposed: `Option<u8>`, 0 = hero). Today's `featured: bool` counts as
   priority 1, and signal's ordered `ParamCuration.featured_param_ids` maps to it directly.
3. Control kind (knob > selector > toggle > meter > readout).
4. Reading order in the source.

Then the form's **budget** is filled:

| Form | Budget | Template |
|---|---|---|
| Pedal Micro | 1 knob + 1 toggle + 1 footswitch | knob top, footswitch bottom |
| Pedal Nano / Standard / Compact | 2–4 knobs (2, triangle 3, 2 × 2) + ≤ 2 mini toggles + LED + 1 footswitch | knobs in the top 45%, label plate, LED, footswitch in the bottom 30% |
| Pedal Wide / Medium | ≤ 6 knobs + ≤ 3 toggles + optional display + 1–2 footswitches | 2 rows of 3 |
| Pedal Large / BigBox | ≤ 10 knobs + display + 2–3 footswitches | display top, 2 rows, switch row |
| Pedal Treadle | 1 continuous (the hero) + toe switch (bypass) | the whole face is the control |
| Rack 1U / 2U / 3U | one row of ≤ 14 / two rows or row + big meter / meter + two rows | left to right in signal order |
| Lunchbox 1 / 2 slots | a column of ≤ 6 knobs + 2 buttons + LED meter / two columns | top to bottom |
| Head | ≤ 12 knobs in a control strip (bottom 22% of a head, top of a combo) | grille or tolex from the source's panel paint |
| Tile n×m | n·m controls, hero first | grid |
| Strip | everything, featured first, one collapsible section per param `group` | one column, 44 pt rows |
| Wide / Square / Full | featured on the first screen, groups as tabs or pages | auto layout |

**Style carries over.** Each picked control is re-placed with the *same kit component id,
variant, ink and legend* as in the source. `knob.knurled_cap` stays a knurled cap, just smaller.
The panel paint is sampled from the source root's fills. For this, placed nodes must record what
built them (§5.3: `Control::kit`, proposed). Without that record, derivation falls back to the
theme's canonical control.

### 4.4 Fitting a view into space

Each view has a `Fit`:

- **`Letterbox`** is the default for physical forms. Uniform scale, centred, never distorted.
  This is what `crates/frame/tools/desktop` `View::fit` does today.
- **`Reflow { min, max }`** is for screen forms (Full, Square, Wide):
  - Between `min` and `max` the root takes the host's size and auto layout re-solves: `wrap`,
    `grow`, and `LayoutChild` min/max, which already exist in `document-proto`.
  - Outside the range it letterboxes at the nearest bound.
  - This needs a runtime layout solver. Today `hug.rs` only computes sizes at authoring time.
- **`Scroll { axis }`** is for Strip. The width fits and the height scrolls. On a scroll gesture,
  a knob drag wins only after the finger moves more along the knob's axis than the scroll axis.
- **`ZoomPan`** is the fallback when a letterboxed view would fall below its touch scale (a 1U
  rack on a phone):
  - Show at the touch scale and pan.
  - Double-tap a section to zoom to it. This is the Neural amp close-up and signal's `ZoomPanel`.

Safe areas are always removed from the host space before fitting.

### 4.5 How a host picks a view

```text
ViewContext {
  space: Size (pt, safe-area removed),
  input: Pointer | Touch | Stage,
  demand: Option<FormClass>,
  prefer: Option<ViewId>,
}
```

`input` sets the minimum hit size: 24 for Pointer, 44 for Touch, 58 for Stage. `demand` is set
by a board: a rack asks for `Rack`.

1. **User choice first.** `prefer`, remembered per host and module (like Neural's last window
   size). It is used if it exists and is usable (step 3).
2. **A demanding board takes an exact class.**
   - A rack takes the view's own U count and allocates that many rows. A 2U face in a rack is
     2U.
   - If the module has no view of that class, the board shows a derived view on the fly
     (step 4), drawn on a generic plate.
3. **Otherwise, score the views:**
   - `scale = fit(view, space)`
   - `usable = scale × min_hit(view) ≥ input.min_hit`, where `min_hit` is the smallest control
     hit box in design px
   - `score = coverage × richness`: the fraction of space used, times the number of bound
     controls
   - Authored views win ties over derived ones.
4. **Nothing usable:**
   - use the `Strip` view if there is one
   - else ZoomPan the best view if its fitted scale is at least half the touch scale
   - else derive a Strip in memory (`derive_view`, not saved)

The chosen `ViewId` survives live reload (§4.8). If the view is deleted, the host chooses again.

### 4.6 Standardized pedals with unique shapes

A `Pedal` form is a **size class plus an optional outline**:

- The size class fixes the **footprint**: the bounding box in mm, what boards pack and snap by.
  It also fixes the **zones**: a bottom band (30% of depth) for footswitches and an LED, an upper
  area for knobs, and jack positions on the sides and top.
- `outline: Option<Outline>` is an SVG path in mm inside the footprint: a rounded wedge, a
  bevelled wah, a triangle. The face draws itself to the outline. Boards use the footprint for
  packing and the outline for shadows and cable exits.
- `Custom { w_mm, d_mm }` is for a pedal that matches no class. It snaps to the nearest class
  for pedal-row alignment, which aligns by the footswitch line so all switches sit on one
  horizontal line, as in Neural's rows.
- `switches: u8` (1–3) declares the footswitch count, so Stomp boards can map them.

**Decided 2026-09-29: pedals share one height.** Every pedal class has the same
height (depth) so a pedalboard row lines up top and bottom, and classes differ
only in **width**: narrow (Micro), standard, wide, double and big-box widths,
with footswitch lines on the same baseline. Treadles (wah, expression) are the
one exception. A custom outline stays inside that fixed height.

### 4.7 Boards

A board is its own document type, a sibling of a surface (§5.4). It holds:

- **slots**: a module reference, which view (form) to use, and a position
- **cables**
- **chrome**: an optional surface id

Arranging:
- **Pedalboard.**
  - Row mode is `Row { order }`: signal order, bottom-aligned, gap 24 px.
  - Board mode is `Free { x_mm, y_mm, rot }` on rails at the board's rail pitch.
  - Cables are drawn from jack to jack as sagging Béziers, coloured by send or return.
- **Rack.** `Rows(u)` counts from the top. Heights come from each view's `units`. Gaps show
  blank panels. Half-width views pair left and right.
- **Chassis.** `Bay(i)`. A double-wide takes `i` and `i + 1`. Past the space, it scrolls
  horizontally.
- **Suite.** `sections: [{ name, icon, board }]`, plus a `chrome` surface: header strip and nav,
  bound to rig-level params (input, gate, transpose, output, preset actions). The section nav
  swaps boards.
- **Module instances and addresses.** A module's bindings are written against its own catalog
  (`fet/input`). A board places *instances*, so each slot has an `addr_prefix`: the engine block
  id, e.g. `chain/3/`. The runtime rebases every binding in the view by it. Without this, two
  1176s on one rack both drive `fet/*`. See §7.

### 4.8 Library, live reload, MCP

**Library.** `SurfaceMeta` gains:
- `views: Vec<ViewMeta { id, name, form, design_size, derived }>`
- a `forms: FormSet` summary (a bitset of `FormClass`)
- one preview PNG per view (`/preview/<id>?view=<view>`)

`ListQuery` gains:
- `forms: Vec<FormClass>`: "modules with a pedal view"
- `complete: bool`: all five ★ forms

The desktop browser adds form chips, and a "Complete" badge on modules with all five ★ forms.
`library-fs` fills `views` from the document (or infers the implicit one). The `.frame`
manifest lists the views, so an index can filter without opening `doc.loro`.

**Live reload.** Nothing changes in `SurfaceStore` or `SurfaceUpdate`. They are still whole
snapshots.
- A host holds `(surface id, view id)`. On an update it swaps the document and re-resolves its
  view by id (`Runtime::swap_surface` on that view's page).
- Several hosts can follow the same document in different views at once (phone Strip, desktop
  Rack), and an agent's edit to any view shows wherever that view is open.
- A `refresh_view` edit to a derived view is just another snapshot.

**MCP / design-session tools.** Each tool is atomic and undoable, one op batch:

| Tool | Does |
|---|---|
| `views` | List views: id, form, size, origin, `min_hit`, and fit per device class |
| `add_view {form, id?, from?: "blank" \| "copy:<view>" \| "derive:<view>", fit?}` | Add a page sized to the form (blank gets a panel from the theme) |
| `derive_view {from, form, pick?: [addr], max?}` | §4.3 derivation; returns an outline and lint |
| `refresh_view {view}` | Re-derive linked, non-detached nodes |
| `set_view {view}` | The session's active view: `describe`, `write`, `add_component` default to its page |
| `set_form {view, form?, fit?}` | Change a view's form or fit (resizes the root, re-snaps a rack to its U) |
| `remove_view {view}` | Remove a view (not the last one) |
| `fit_check {view?, device?}` | Per device (desktop 1920, iPad landscape and portrait, iPhone landscape and portrait): scale, usable or not, which controls fail |

`catalog` also prints param priority and role, so a model knows what a pedal should keep.

**Lint rules** (`design-lint`):
- **`view-size`**: the root size matches the form (±1 px, or inside the Reflow range).
- **`view-touch`** (per view): `min_hit × fit scale` on the view's primary device is at least
  the device floor. Replaces the global `TOUCH_MIN` for views.
- **`pedal-zones`**: a footswitch sits in the bottom band, bound to a bypass param or an action.
  Knobs are not in the band.
- **`view-drops-featured`** (info): a derived view drops a priority-0/1 param that was in budget.
- **`module-complete`** (info): lists the ★ forms still missing.

### 4.9 Embeds: a module in someone else's strip

Built in `crates/frame-embed` (a `crates/` library: it is what a host reaches for, and it
composes surface-runtime, surface-kit, surface-widgets and the render collector behind one
API; it has no wire contract or backends of its own).

**Tiers.** An embed sheds detail by width, at session's thresholds (`tone::Rack::at`), so a
frame panel and the hand-drawn rack it replaces change at the same widths:

| Tier | Width | Shows |
|---|---|---|
| `Focus` | ≥ 260 | the plugin's own editing surface: its grid, labels, bigger nodes, times in the header |
| `Full` | ≥ 96 (`LEGIBLE`) | every panel with its grid, decade labels, handles, the zoom chip |
| `Curves` | ≥ 80 (`SHAPE`; REAPER's 86 px strip lands here) | the curves alone |
| `Minimal` | ≥ 16 | a rail: the same rows at the same heights, one live indicator each |
| `Off` | < 16 | nothing; the host keeps the room |

A selected strip opens to 133 (`WORKING`). Row heights do not change with the tier.

**Views are pages** (§4.2) until `Scene.views` exists: an authored embed lives on a page named
`embed:strip`, `embed:mcp` or `embed:tcp`, optionally one page per breakpoint
(`embed:mcp@96` is shown from 96 px up). The widest breakpoint that fits wins, then the host's
plain page, then `embed:strip`, then the first page; the page is scaled to the host's width.
Views built in code (`EmbedView`) are rebuilt for every new box instead — that is the reflow
until a runtime layout solver exists (Phase E).

**Painting, in two passes** (session's mixer model):
- `record`: everything that depends only on params — the ground, grids, curves (from the
  curve registry), handles, labels — painted once into the host's own scene and replayed under
  the strip's transform until `needs_record()`.
- `live`: everything that moves with the audio or with time — plots bound to feeds, meters,
  curves reading a feed, motion — painted over the top every frame while `animating()`.

It paints *into the host's scene*: `Canvas` is anyrender's `PaintScene` shape in kurbo 0.13 /
peniko 0.6. `AnyRender` (feature `anyrender-0-11`) wraps session's anyrender 0.11 `Scene`;
`Recording` is frame's replayable list; `cpu::Rasterizer` (vello_cpu) rasterises for a
bitmap host. Text is drawn as glyph outlines so it replays anywhere.

**Values the controls cannot say** (a header's "4 · 5.5dB", a band handle's hue, the band
between a gate's two lines) are `Derived` properties: functions of the values, applied to the
frame being painted. Params a layout depends on (an EQ's zoom, which machine a delay is, which
phases are folded) trigger a rebuild, never mid-gesture.

**Pointer**, strip-local: drag, wheel, double-click (reset) and cmd/ctrl-click (reset) go to
the runtime; a right click is `passthrough` (the host's context menu).

**REAPER** (`nice_plug::FrameEmbeddedEditor`, feature `nice-plug`): REAPER asks for size hints
(`GETMINMAXINFO`: min/max px, preferred and minimum aspect as 16.16), then paints on its UI
timer (about 30 Hz, `PAINT_OPTIONAL` unless forced) into a LICE bitmap of the slot's device
pixels (BGRA words, a row span, maybe bottom-up; DPI in 24.8 fixed point, retina doubles the
pixels). Mouse messages are slot-local device px with automatic capture; `INVALIDATE` forces a
repaint. The editor rasterises on the CPU straight into the bitmap — an MCP-sized frame is
0.1–2.2 ms in release, cheaper than a GPU readback — and skips optional paints when nothing
moved.

**session's rack** is `tone::ToneRack` (a strip's whole chain, phase containers included) and
`tone::TonePanel` (one unit), with the same geometry and inks as `apps/session-daw/src/tone.rs`;
the curve maths is session's by override — the host registers `eq_ui`'s
`calculate_combined_response` as `eq.response`, `comp_ui`'s `compress_transfer` as
`comp.transfer`, `saturate_dsp`'s `transfer_curve` as `sat.transfer`, its rendered reverb tail
as `room.tail`. `fx_slot::FxProviders` is the framework-free half of daw-ui's
`EmbeddedFxGuis` seam. Renders: `examples/embed/` (`cargo run -p frame-embed --example strips
-- --compare <session golden png>` rebuilds them).

---

## 5. Types and where they live

Per the architect layout, the pure data types go in the protos, the algorithms go in a new
surface crate, and boards get their own feature.

### 5.1 `surface-proto::form`: pure data, standards

Location: `features/surface/surface-proto/src/form.rs`.

`document-proto` already depends on `surface-proto`, so the form types live on the lower layer.

```rust
pub const DESIGN_PX_PER_INCH: f64 = 60.0;
pub const DESIGN_PX_PER_MM: f64 = DESIGN_PX_PER_INCH / 25.4;
pub const RACK_UNIT_IN: f64 = 1.75;
pub const RACK_WIDTH_IN: f64 = 19.0;
pub const RACK_PANEL_CLEARANCE_IN: f64 = 1.0 / 32.0;
pub const LUNCHBOX_SLOT_IN: (f64, f64) = (1.5, 5.25);
pub const EURO_HP_MM: f64 = 5.08;
pub const EURO_3U_MM: f64 = 128.5;

#[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
#[repr(u8)]
pub enum FormFactor {
    Full,                       // 1600×900 pt
    Square,                     // 960×960 pt
    Wide,                       // 800×400 pt
    Strip,                      // 390 pt wide, scrolls
    Tile { cols: u8, rows: u8 },
    Pedal(PedalForm),
    Rack(RackForm),
    Lunchbox { slots: u8 },     // 1..=3
    Head(HeadForm),
    Euro { hp: u16 },
    Embed(EmbedForm),           // the host's box; frame-embed::form today
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Facet, Serialize, Deserialize)]
#[repr(u8)]
pub enum FormClass { Full, Square, Wide, Strip, Tile, Pedal, Rack, Lunchbox, Head, Euro, Embed }

#[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
pub struct PedalForm {
    pub size: PedalSize,
    /// SVG path in mm, origin top-left of the footprint.
    pub outline: Option<String>,
    pub switches: u8,           // 1..=3
}

#[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
#[repr(u8)]
pub enum PedalSize {
    Micro, Nano, Standard /* the default */, Compact, Wide, Medium, Large, BigBox, Treadle,
    Custom { w_mm: f64, d_mm: f64 },
}

#[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
pub struct RackForm { pub units: u8, pub width: RackWidth }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
pub enum RackWidth { Full, Half }

#[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
pub struct HeadForm { pub kind: HeadKind, pub w_mm: Option<f64>, pub h_mm: Option<f64> }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
pub enum HeadKind { Head, Combo, Cab, Tabletop }

impl FormFactor {
    pub fn class(&self) -> FormClass;
    /// Canonical design size (w, h) in design px (§3.1).
    pub fn design_size(&self) -> (f64, f64);
    /// Physical face size in mm, for physical forms.
    pub fn physical_mm(&self) -> Option<(f64, f64)>;
    pub fn default_fit(&self) -> Fit;
    /// Best guess from a root frame's size (implicit views of old documents).
    pub fn infer(w: f64, h: f64) -> Self;
    /// The `form="rack:2u"` markup spelling, both ways.
    pub fn to_markup(&self) -> String;
    pub fn parse(s: &str) -> Option<Self>;
}

#[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
#[repr(u8)]
pub enum Fit {
    Letterbox,
    Reflow { min: (f64, f64), max: (f64, f64) },
    Scroll { vertical: bool },
    ZoomPan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet, Serialize, Deserialize)]
#[repr(u8)]
pub enum InputClass { Pointer, Touch, Stage }

impl InputClass {
    pub fn min_hit(self) -> f64 { match self { Self::Pointer => 24.0, Self::Touch => 44.0, Self::Stage => 58.0 } }
}

// Built today in `crates/frame-embed/src/form.rs`; moves here unchanged.
pub struct EmbedForm { pub host: EmbedHost }
pub enum EmbedHost { Strip, Tcp, Mcp }                 // page names embed:strip / embed:tcp / embed:mcp
pub enum Tier { Focus, Full, Curves, Minimal, Off }    // Tier::at(width): 260 / 96 / 80 / 16
pub struct SizeHints { pub preferred_aspect: f64, pub minimum_aspect: f64, pub min: (f64, f64), pub max: (f64, f64) }
```

Additions to `param.rs` (coordinate with signal's `ParamCuration`):

```rust
pub struct ParamInfo {
    …,
    /// 0 = hero; lower = more important. `featured` ⇒ Some(1) when unset.
    #[serde(default)] pub priority: Option<u8>,
    #[serde(default)] pub role: Option<ParamRole>,
}
#[repr(u8)] pub enum ParamRole { Bypass, Hero, Output, Input, Mix, Tap, Tempo, Tuner }
```

Addition to `control.rs`, so derivation can re-place the same component:

```rust
pub struct Control { …, #[serde(default)] pub kit: Option<KitRef> }
pub struct KitRef { pub id: String, pub variant: Option<String>, pub size: f64, pub ink: Option<String> }
```

### 5.2 `document-proto`: views on the scene

Location: `features/document/document-proto/src/view.rs`, plus a field on `Scene` and ops in
`ops.rs`.

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Facet, Serialize, Deserialize)]
pub struct ViewId(pub String);            // "full", "rack", "pedal", "strip", …

#[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
pub struct View {
    pub id: ViewId,
    pub name: String,
    pub page: NodeId,
    pub form: FormFactor,
    pub fit: Fit,
    pub origin: ViewOrigin,
}

#[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
#[repr(u8)]
pub enum ViewOrigin {
    Authored,
    Derived {
        from: ViewId,
        recipe: DeriveRecipe,
        /// Derived node → source node, for refresh.
        links: BTreeMap<NodeId, NodeId>,
        /// Hand-edited since derivation; refresh leaves these alone.
        detached: BTreeSet<NodeId>,
    },
}

#[derive(Debug, Clone, PartialEq, Facet, Serialize, Deserialize)]
pub struct DeriveRecipe { pub pick: Vec<ParamAddr>, pub max: Option<u32>, pub template: String }

pub struct Scene { …, #[serde(default)] #[facet(default)] pub views: Vec<View> }

impl Scene {
    /// Declared views, or the implicit one (first page, inferred form).
    pub fn views(&self) -> Cow<'_, [View]>;
    pub fn view(&self, id: &ViewId) -> Option<&View>;
    /// The view's root frame (the page's first child).
    pub fn view_root(&self, id: &ViewId) -> Option<NodeId>;
}

// ops.rs
pub enum Op { …, PutView { view: View }, DropView { id: ViewId } }   // inverses of each other
```

`Set` on a node inside a derived view also adds it to `detached` (the editor's and session's
`commit` does this, not the op itself). `document-text` prints and parses the `<Page view= form=
fit= derived-from=>` attributes; the recipe, links and detached set go in a
`<View-derivation>` child element so the round trip stays lossless. `document-bundle`'s
`Manifest` gains `views: Vec<ViewMeta>`.

### 5.3 New crate: `features/surface/surface-views`

This crate holds the algorithms. It depends on `document-proto`, `surface-proto`,
`surface-widgets` and `surface-kit`.

```rust
pub struct ViewContext { pub space: (f64, f64), pub input: InputClass, pub demand: Option<FormClass>, pub prefer: Option<ViewId> }
pub struct Choice { pub view: ViewId, pub placement: Placement, pub derived_on_the_fly: Option<Scene> }
pub struct Placement { pub scale: f64, pub offset: (f64, f64), pub size: (f64, f64), pub scroll: Option<(f64, f64)> }

pub fn choose_view(scene: &Scene, catalog: &Catalog, ctx: &ViewContext) -> Choice;   // §4.5
pub fn place(view: &View, root_size: (f64, f64), space: (f64, f64)) -> Placement;      // §4.4
pub fn min_hit(scene: &Scene, view: &ViewId) -> f64;
pub fn fit_report(scene: &Scene, view: &ViewId) -> Vec<DeviceFit>;                       // fit_check
pub fn derive_view(scene: &Scene, catalog: &Catalog, from: &ViewId, form: FormFactor, opts: &DeriveOpts) -> Result<Vec<Op>, DeriveError>;
pub fn refresh_view(scene: &Scene, catalog: &Catalog, view: &ViewId) -> Result<Vec<Op>, DeriveError>;
pub mod templates { pub fn pedal(..); pub fn rack(..); pub fn lunchbox(..); pub fn head(..); pub fn tile(..); pub fn strip(..); }
pub mod reflow { pub fn solve(scene: &Scene, root: NodeId, size: (f64, f64)) -> Vec<(NodeId, Rect)>; }   // Phase 4
pub const DEVICES: &[Device];   // desktop-1080, desktop-1440, ipad-l, ipad-p, iphone-l, iphone-p, with safe areas
```

Callers:
- `design-session` gains the §4.8 tools.
- `design-lint` gains the view rules.
- The desktop player gains `--view <id>` and `--device <name>`, plus a view switcher (`V`).
- `render_surface` gains `--view` and `--all-views`, which writes a contact sheet.

### 5.4 New feature: `features/board`

| Crate | Contents |
|---|---|
| `board-proto` | `Board { id, kind: BoardKind, slots: Vec<Slot>, cables: Vec<Cable>, chrome: Option<SurfaceId> }`; `BoardKind { Pedalboard { w_mm, d_mm, rails_mm: Vec<f64>, row: bool }, Rack { units: u16, width: RackWidth }, Chassis { slots: u8 }, Stage, Suite { sections: Vec<Section> }, Stomp { cols: u8, rows: u8, hold_ms: u32 }, Perform, Keys }`; `Slot { module: SurfaceId, view: Option<ViewId>, addr_prefix: Option<String>, at: SlotPos }`; `SlotPos { Row(u32), Free { x_mm, y_mm, rot_deg }, Rows(u16), Bay(u8) }`; `Cable { from: (usize, Jack), to: (usize, Jack), color }`; a `BoardStore` rpc like `SurfaceStore` |
| `board-layout` | `arrange(board, library) -> Arranged { per slot: view + Placement, plus board chrome nodes }`: pack, snap and draw rails, ears and cables. Composes one scene the runtime can play, with each slot's bindings rebased by `addr_prefix` |
| `board-memory` | in-memory store for tests |

Boards are layout and cabling only. Signal order and routing stay in the engine (signal's rig
graph); a board only draws them.

---

## 6. Phased plan

**Phase A: views exist.** Touches `surface-proto`, `document-proto`, `document-text`,
`document-bundle` and the desktop player.
- Add `form.rs`, `View`, `Scene.views`, `PutView` / `DropView`, and the markup attributes (with
  a round-trip law test).
- Add the implicit view with `FormFactor::infer`, and the manifest views.
- Desktop: `--view`, a `V` key to cycle views, and a letterbox fit per view.
- **Done when:** a hand-written two-page `.frame` (Rack 2U + a hand-drawn Standard pedal of the
  1176) plays both views, and every existing example is unchanged.

**Phase B: the library knows forms.** Touches `library-proto`, `library-fs`,
`frame-library-server` and the desktop browser.
- Add `ViewMeta`, `FormSet`, `ListQuery.forms` / `complete`.
- One preview per view, form chips, and the "Complete" badge.

**Phase C: derivation.** Touches the new `surface-views`, `surface-kit` and `design-session`.
- `ParamInfo.priority` / `role`, and `Control.kit` recorded by `surface_kit::place`.
- Templates for Pedal (all classes), Lunchbox, Tile and Strip.
- `derive_view` / `refresh_view`, the MCP tools, and the lint rules.
- `plugin-faces`:
  - every face declares its authored Rack view: 1176 2U, Pultec-style 3U, API-style 1U
  - derive Pedal, Lunchbox, Tile and Strip views for each face
  - `cargo run -p plugin-faces` writes all views
- **Done when:** `derive_view rack→pedal` on the 1176 face gives a Standard pedal with Input,
  Output, Ratio and a bypass footswitch, in the same knob style, lint-clean.

**Phase D: fitting and choosing.** Touches `surface-views` and the hosts.
- `choose_view`, the Scroll and ZoomPan fits, safe areas, `fit_check`, and `DEVICES`.
- Desktop `--device iphone-p` simulates the device.
- The iOS host (signal-frame's airlock app) passes `ViewContext` from its window and traits.

**Phase E: reflow.** Touches `surface-views::reflow` and `surface-runtime`.
- A runtime auto-layout solver, and `Fit::Reflow` for Full, Square and Wide.
- Authored Full and Square views for two faces.
- Head templates, and Wide derivation from Full.

**Phase F: boards.** Adds `features/board` and `crates/frame/tools/desktop --board`.
- Pedalboard row and board modes, and Rack, Chassis and Stage boards.
- Address rebasing and cables.
- Suite with a chrome surface.
- **Milestone:** a Neural-style suite (pre-FX pedals, amp Stage, cab, EQ rack, post-FX) built
  from plugin-faces and mock catalogs, playable on desktop and iPad.

**Phase G: signal.**
- Stomp board replaces `PerformGrid` (5 × 2, tap/hold, chords).
- Perform board with Tiles, keys mixer from Lunchbox views, and iPhone Strip views.
- Signal publishes `priority` / `role` from `ParamCuration`.

**Phase H: embeds.** `crates/frame-embed` (built 2026-09-29, §4.9).
- Done: the `Embed` form and tiers; `Embed` (reflow per box, record + live passes, pointer
  routing, `Derived` values, rebuild on layout params); `Canvas` into anyrender 0.11, a
  `Recording` or vello_cpu; session's Tone rack as `ToneRack` / `TonePanel` (EQ, gate,
  compressor, saturator, de-esser, delay, reverb; phase containers and folds); a featured-param
  embed for any catalog; `DocumentView` for authored `embed:*` pages; the nice-plug
  `EmbeddedEditor` for REAPER's TCP/MCP; a headless harness with REAPER's sizes and timings.
- Next: session replaces `tone::record`/`draw` with `ToneRack` (its `Tone` store published as
  `<unit>/<param>` values, its meters as feeds, its plugin maths registered as curve
  providers); `FormFactor::Embed` moves into `surface-proto::form` with Phase A; a runtime
  reflow solver (Phase E) lets authored `embed:*` pages reflow instead of scale; derived embed
  views (`derive_view … Embed`) from a module's Rack view.

Phases A and B are small and unblock everything else. C is the biggest win: every module gets
four more forms for free. E (reflow) is the riskiest and can come last among the module-level
phases.

---

## 7. Decisions

Decided by the user on 2026-09-29:
- **Views are pages in one document** (decision 2): accepted.
- **Derived views are saved and editable** (decision 3): accepted.
- **Forms are optional per module:** a module supports only the forms that are
  relevant to it. None is mandatory, so there is no "complete" requirement and no
  `module-complete` lint error; it is at most an informational badge.
- **Accepted module forms:** Full, Square, Pedal, Rack, 500-series, plus Head,
  Strip, Wide, Tile and Eurorack.
- **Pedals:** custom outlines are allowed. All pedals share one standard height, and
  classes vary by width (see 4.6).

Still open (defaults used unless the user says otherwise): 60 px/in density (1),
the new `ParamInfo.priority`/`role` and `Control.kit` fields (5), instance address
prefixes on boards (7), and a single plugin's Full view (9).

### Original list


1. **Design density of 60 px/in.** This gives round rack and 500-series numbers (1U = 105,
   19 in = 1140, slot = 90 × 315). Today's 1176 face is 1120 wide (≈ 59 px/in), which is within
   2%. The alternative is to keep each face's own scale and store only the physical size.
2. **Views are pages in one document**, not separate documents per form. One catalog, one
   library entry, one live-reload stream. The cost is a bigger document per module.
3. **Derived views are materialized** (editable nodes with links and refresh), not generated at
   runtime. Runtime derivation remains the fallback only.
4. **What "complete" means.** The proposal is the user's five (Full, Square, Pedal, Rack,
   Lunchbox). Strip and Tile are recommended but derivable, so they don't count.
5. **New surface-proto fields.** `ParamInfo.priority` and `role`, and `Control.kit`. These touch
   crates other agents are editing now, so they need a coordinated change. `priority` should
   match signal's `ParamCuration.featured_param_ids` order.
6. **Add Head, Strip, Wide and Tile as module forms, and Euro as optional?** And is Eurorack
   worth doing at all, given its only consumer would be keys-rig synth modules?
7. **Address rebasing (`addr_prefix`) for module instances on boards.** Is the instance prefix
   the engine's block id, or something the board assigns and the engine resolves?
8. **Pedal outlines.** Is a free SVG outline allowed inside a size-class footprint (proposed), or
   are only the classes allowed?
9. **What a single plugin's Full view shows.** For a rig, Full is the Suite. For one plugin
   (e.g. the 1176) the proposal is: the face letterboxed on a stage backdrop, plus a larger
   meter or visualizer panel (viz kit), a preset bar and an A/B toggle. That is the Neural
   window pattern applied to one unit.

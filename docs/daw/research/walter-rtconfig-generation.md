# WALTER, rtconfig.txt, and what happens to `daw-ui/src/panels/`

Research for [#128](https://github.com/FastTrackStudios/FastTrackStudio/issues/128)
(*How REAPER's height-driven section collapse becomes a responsive layout*) and
[#130](https://github.com/FastTrackStudios/FastTrackStudio/issues/130)
(*What happens to WALTER when daw-ui's panels are deleted*). They are one
decision: **does the Dioxus layout mirror REAPER's, or does an exporter
translate ours back into WALTER?** Everything else — hand-maintained vs
generated `rtconfig.txt`, delete vs keep the panels — falls out of that.

Constrained by the closed decisions on
[#125](https://github.com/FastTrackStudios/FastTrackStudio/issues/125) (the
slice model) and
[#126](https://github.com/FastTrackStudios/FastTrackStudio/issues/126) (Tailwind
outside `<svg>`, presentation attributes inside), and by #124's standing rule:
WALTER is authoritative on **structure**, never on numbers.

---

## RECOMMENDATION

**Mirror REAPER's breakpoints in the Dioxus panels. Keep `rtconfig.txt` a
hand-maintained fork — but generate the thresholds into it, and nothing else.
Keep `features/daw-ui/daw-ui/src/panels/`.**

Concretely, three things:

1. **Container queries at REAPER's own thresholds** (260 / 320 / 350 / 400 for
   the pan/label/input/input-FX collapse, plus the derived-gap ones below). The
   numbers are already in the file; encoding them is data entry, not design.
2. **One narrow generator**: the five `set hide_* N` lines in `initTrackGlobals`
   (`rtconfig.txt:308-312`) and the three section heights
   (`rtconfig.txt:2126-2128`) get emitted from a single Rust `Breakpoints` const
   that the Dioxus panels also read. That is the *only* place the two sides can
   silently disagree about #128's question, and `fts_themer::rtconfig` already
   does exactly this kind of surgical splice
   (`features/reaper/fts-themer/src/rtconfig.rs:70-129`). Everything else in the
   3743-line file stays hand-edited.
3. **Do not build the full layout compiler now.** Its payoff window is exactly
   the period when the two layouts agree — i.e. now — and during that window its
   payoff is nearly zero. See "Sizing the generator" and the counter-argument.

Rough cost: option A is **~1–2 sessions**. Option B (full generation) is
**several weeks and must land at ~100%**, because REAPER has no partial mode —
one wrong `h<` conditional and the strip visibly breaks at some height, on
someone's machine, at some panel size we never sampled.

---

## 1. What `rtconfig.txt` actually expresses

`features/reaper/fts-theme/FastTrackStudio/rtconfig.txt` — **3743 lines**, 2855
of them non-blank non-comment.

| construct | count | what it is |
|---|---|---|
| `set` | 1524 | the only assignment. Evaluates a prefix-notation expression into a coordinate list |
| `Layout` / `EndLayout` | 117 each | named layout scopes; `Layout "A_Fader_Blue" "blue"` also names an image sub-folder |
| `layout_dpi_translate` | 101 | 100 / 150 / 200 % tier wiring |
| `define_parameter` | 91 | author-exposed knobs, read by REAPER's Theme Adjuster |
| `custom` | 74 | declares an extra drawable, optionally bound to an action id and an image name |
| `clear` | 57 | wipes a namespace (`clear mcp.*`) at the top of each layout |
| `macro` … `endmacro` | 39 | textual, with `##` token concatenation |
| `front` | 20 | z-order |

Plus the value language itself: the 8-tuple `[x y w h ls ts rs bs]` (px box +
four per-edge attach scales), `{n}` list indexing, `expr@slot` sparse placement,
`.` meaning "this element's current value", `[0]` meaning hidden, prefix
arithmetic (`+ - * /`), and strictly binary Polish conditionals
(`h<260 A B`, nestable in either answer).

**Section sizes.** The MCP is `rtconfig.txt:2073-2996` — 924 lines, 716 of them
code, **410 `set` statements** across **106 distinct `mcp.*` attributes**. The
two macros that matter are `drawMcp` (`2082-2454`, 373 lines) and `drawMcpStrip`
(`2456-2596`, 141 lines). The MCP is thus roughly a quarter of the file; the TCP
(`356-1710`) is larger and harder.

A generator would have to emit: `set`, the 8-tuple, `@`, `{n}`, `.`, `[0]`,
prefix arithmetic, binary conditionals, `custom` (with action ids and image
names), `front`, `clear`, `Layout` nesting, and the DPI tier wiring. Macros and
`define_parameter` it could skip by inlining — at the cost of a file no human
can maintain afterwards, which forecloses the fallback.

Worth knowing what "emit WALTER" means in practice: the language has **no
`floor()`**. The theme implements one as a 66-line six-deep binary search over
0..64 (`rtconfig.txt:492-557`). A generator that ever needs to round has to emit
that, or something like it.

---

## 2. How much is height-driven collapse

Most of the interesting part. The MCP's five global thresholds are declared
once, in px at 100 % (`rtconfig.txt:308-312`), then scaled per DPI tier
(`2089-2093`):

```
hide_inputFX        400   ; input-FX row
hide_input          350   ; record-input dropdown
hide_pan_labels     320   ; pan/width text labels
hide_pan            260   ; pan controls (as their own section)
hide_volume_label   250   ; the fader's dB readout
```

They drive the **section stack** (`2126-2128`), which is the responsive core:

```
fx_sec   = 33
pan_sec  = 6  below 260  |  33 with labels hidden  |  50 with labels
in_sec   = 22 below 350  |  42 below 400           |  54 otherwise
```

`stretch_sec` (`2135-2137`) then absorbs whatever is left after `bot_sec`, and
that is what the meter and fader live in. So the strip is: three
height-quantised bands on top, one flexible band, one fixed band at the bottom —
`flex-column` with three `flex-basis` steps and one `flex-grow: 1`, expressed as
arithmetic.

**What re-anchors.** Below `hide_pan` (260) the pan section is gone but the pan
control is not: it re-parents onto `mcp.recmode` inside `in_sec`
(`2301-2305`), and `mcp.recmode` itself then hides (`2307`). That is the case
#128 names, and it is genuine re-anchoring, not just repositioning.

**Thresholds on derived space, not on the container.** The rest are gated on
`stretch_sec_h` — the residual after the fixed bands — not on `h`:

| element | condition | source |
|---|---|---|
| `mcp.io` | `stretch_sec_h < 106` | `2341-2342` |
| `mcp.env` | `stretch_sec_h < 125` (labels off) / `< 141` | `2343-2345` |
| `mcp.phase` | `stretch_sec_h < 144` / `< 162` | `2346-2348` |
| `padding` | steps 4 → 3 → 2 px at `stretch_sec_h < 350` / `< 250` | `2140-2141` |
| fader ↔ meter swap | `h >= 280` in narrow mode | `2331-2333` |

The strip layout goes further and thresholds on the gap **between two resolved
elements**:

```
pan_area   = mcp.recarm.y − (mcp.fx.y + mcp.fx.h)      ; 2529
fader_area = mcp.label.y  − (mcp.io.y  + mcp.io.h)     ; 2564
```

with four tiers on `pan_area` (20 / 45 / 75 / 90 px, `2530-2533`) moving and
hiding pan, width and their labels (`2536-2560`), and two on `fader_area`
(125 / 180) hiding env and phase (`2569-2573`). Then the fader takes the
remainder (`2576-2585`) and, below 45 px, **changes widget type** —
`mcp.volume.fadermode 1` turns the fader into a knob (`2587-2593`).

The TCP is width-driven rather than height-driven, and worse: `calcTcpFlow`
(`1415-1436`) runs thirteen elements through the `then` macro (`740-780`), which
is a real flow engine — place after previous, wrap to next row when
`this_end > main_eff`, cull when it still does not fit, cull again when the row
would exceed the panel height, and track where each row ended. Feeding it is
`shrinkLabelAndVolume` (`374-402`), a two-variable proportional shrink solver
called **three times** at successively lower floors (72/72, 48/40, 40/40 —
`660`, `667`, `685`), each pass gated on whether the next feature (the side
lists, the extra FX/IO button, the flipped label) has to be dropped.

The transport (`3204-3442`) is the same idea on the width axis: fixed element
sizes (`3213-3226`), summed section widths, and four row-height tiers
(`3235-3238`).

**Verdict for #128:** REAPER's collapse is *not* one breakpoint ladder. It is
(a) five container-height thresholds, (b) six thresholds on a derived residual,
(c) six on the gap between two siblings, (d) a widget-type switch, and (e) on
the TCP, a wrap-and-cull flow with a three-stage shrink solver. Container
queries at the same thresholds reproduce (a) directly and (b)/(c) only if the
Dioxus tree is structured so those residuals *are* containers — which is the
real design work in mirroring, and it is worth doing because it is also just a
good flex-column decomposition.

---

## 3. The expressiveness gap, both ways

### WALTER can say things CSS/Taffy cannot

1. **Arithmetic on another element's resolved box.** `set mcp.recmon + + [0
   padding] [mcp.recarm mcp.recarm] * scale [7 20 21 20]` (`2337`) — and the
   chain continues through mute (`2339`), solo (`2340`), io (`2342`), env
   (`2345`), phase (`2348`). CSS has no "place me relative to that box over
   there"; anchor positioning is not in the pinned Blitz/Stylo.
2. **Thresholds on a derived gap** (`pan_area`, `fader_area`, `stretch_sec_h`).
   A container query can only query a container. This is expressible only if the
   gap is made into a box — a structural constraint on our tree, not a free
   translation.
3. **Widget-type switching from geometry** — `fadermode 1` below 45 px
   (`2592`). Ours would be a Rust `if`, not CSS.
4. **Colour computed from colour.** `get_luma` (`298-300`) picks bright vs dark
   text against a tinted background (`2179-2181`, `2385-2387`), and the whole
   track-colour tint/dim/select chain (`2144-2156`). No CSS function does this
   in Stylo. Not a problem for us — #124 already forbids `currentColor` and
   requires Rust-computed literals — but a *generator* would have to emit these
   as WALTER expressions, and there are ~40 `.color` assignments in the MCP
   alone.
5. **Host state we do not have a box for**: `trackpanmode` (3/5/6 change the
   pan/width control entirely), `folderstate`/`folderdepth`/`maxfolderdepth`,
   `mixer_visible`, `reaper_version`, `os_type`, `tracknch`.

Most of (1)–(3) a generator can emit mechanically *if it has measured the
result*. That is the point: WALTER's extra power here is arithmetic, and
arithmetic is what generators are good at.

### Dioxus/Tailwind can say things WALTER cannot

1. **Content measurement.** Taffy measures text; WALTER cannot. The theme's own
   evidence: `tcpLabelAutoMeasured` is a *parameter fed in from REAPER's
   adjuster* (`1445`), and the label-flip threshold carries the comment
   `; tune 54 by eye. Sorry.` (`688`). Any `fit-content` / `max-content` /
   ellipsis decision in our layout is **not exportable**.
2. **Real flex/grid semantics** — `gap`, `space-between`, wrap driven by
   measured content, grid auto-placement. Reducible to absolute coords *at one
   size*; not reducible to WALTER's resize model in general (see below).
3. **Nonlinear response to container size.** WALTER interpolates each edge
   linearly from the natural size via `ls ts rs bs`
   (`features/daw-ui/daw-ui/src/theming/walter.rs:84-97`). Anything that clamps,
   wraps or hits a min/max between two of our samples has to become an *explicit
   conditional* the generator discovered. Miss one and the theme drifts at
   exactly the sizes we did not sample.
4. **New controls.** This is the one-way part, and it is decisive. REAPER draws
   its panels from a **fixed element vocabulary** — `mcp.volume`, `mcp.meter`,
   `mcp.recarm`, `mcp.pan`, … There is no way to add a fourteenth element with
   behaviour. `custom` (74 uses) gets you an extra *image*, optionally bound to
   an action id (`custom mcp.custom.folder_overlay "" 2000`, `2446`;
   `custom tcp.custom.fixed_lanes_off '' 42430 'Lanes'`, `942`) — a picture and
   a click, not a fader, meter or text field.
5. **Structure**: nesting, overflow/scroll, border-radius, ellipsis, hover,
   transitions. WALTER is flat panel coordinates and hover-by-sprite-strip.

### Is the gap one-way?

**No — it is two-way, but asymmetric, and the asymmetry is what decides this.**

WALTER's surplus is arithmetic a generator can emit. Our surplus is *content
measurement and new controls*, which WALTER structurally cannot host at any
price. So full generation is available **only for layouts built from REAPER's
existing element vocabulary and free of content-driven sizing** — i.e. only for
layouts that already look like REAPER's.

Which means: while the two sides agree, generation is possible but buys almost
nothing (we would be generating a file we already have). The moment we make a
design decision REAPER cannot express, generation stops being possible for that
decision — and we are hand-authoring the REAPER side anyway. **Both options are
technically available today; option B's window of usefulness is the window in
which it is least useful.**

---

## 4. What exists today

### Nothing generates WALTER. Something *executes* it.

The find that changes the shape of #130:
`features/daw-ui/daw-theme-reaper/src/walter.rs` is a **938-line WALTER
interpreter** — preprocessor (comments, `\` continuations, tokenizer), `macro`
expansion with `##`, `def` substitution, prefix arithmetic, binary conditional
chains, `{n}`, `@slot`, `Layout` scoping, `clear`, `custom`, `front`. Entry
point `evaluate(src, layout, env)` at `walter.rs:134`; the environment is ~30
REAPER scalars (`walter.rs:57-97`).

It is corpus-tested against four real third-party themes — Anti-Theme,
Reapertips, Neptune VI, Imperial
(`features/daw-ui/daw-theme-reaper/tests/corpus.rs:8-13`,
`tests/antitheme.rs`).

Around it: `daw-theme-reaper/src/rtconfig.rs` (globals + `define_parameter`
parsing), `palette.rs` (COLORREF), `images.rs` (608 lines of atlas slicing,
including the magenta/yellow marker geometry).

### The panels consume it at runtime

`features/daw-ui/daw-ui/src/theming/mcp.rs:394-434` defines `LayoutEngine` —
`(ctx, layout, w, h, StripState) → McpLayout`, with the doc comment already
stating the thesis of #128:

> REAPER re-runs WALTER on every panel resize — flow-based themes (Reapertips'
> `then`-macro chain) reorder, wrap, shrink and cull elements per the *actual*
> panel size, which a one-shot anchor bake cannot reproduce.

`features/daw-ui/daw-ui/src/panels/mcp_strip.rs:68-84` calls
`engine.layout_at(...)` with the strip's real px box, per frame. So the panels
are **not** a hardcoded imitation of a REAPER layout — they are a WALTER host.
`features/daw-ui/daw-ui/src/theming/walter.rs` (327 lines) carries the typed
`Coord`/`Margin`/`FontSpec`/`ThemeParam` model and both resolvers
(`resolve` → px rect, `css_position` → `calc()` absolute placement).

### The panels themselves

`features/daw-ui/daw-ui/src/panels/` — 10 files, **3968 lines**:

| file | lines | |
|---|---|---|
| `arrange_view.rs` | 1499 | TCP sidebar + timeline lanes |
| `mcp_strip.rs` | 1194 | the WALTER-laid-out strip (MCP *and* TCP contexts) |
| `model.rs` | 301 | `TrackView` etc. — host-agnostic on purpose |
| `envcp_row.rs` | 205 | |
| `transport_bar.rs` | 215 | |
| `mixer_control_panel.rs` | 166 | |
| `track_control_panel.rs` | 168 | |
| `native.rs` | 109 | the *vector* path — "no WALTER, no bitmaps" |
| `workspace.rs` | 76 | arrange-over-mixer composition |
| `mod.rs` | 35 | |

Two consumers, and they are different in kind:

- `apps/fasttrackstudio/src/mixer_view.rs:278` — `MixerControlPanel` in the app.
  This is the one the Dioxus rewrite replaces.
- `features/reaper/fts-themer-ui/src/preview.rs:30-38` — `DawWorkspace` under a
  theme rebuilt live from `.ReaperTheme` + `rtconfig.txt` *text*, with no
  filesystem, so a colour edit re-derives the preview on the same frame. **This
  is already the third-party theme viewer #130 asks whether to build.**

### The exporter and the only rtconfig tooling

`features/daw-ui/daw-theme-art/src/export.rs` (1139 lines) is art-only — cells,
markers, compositing. It has no concept of layout and never touches
`rtconfig.txt`.

`features/reaper/fts-themer/src/rtconfig.rs` (245 lines) does exactly one thing:
splice `Layout "A_Fader_<Name>"` blocks into the three DPI tiers for a generated
accent, preserving indentation and idempotent. Its module doc is the current
policy, stated plainly:

> `rtconfig.txt` is hand-written WALTER with meaningful comments and formatting,
> and it is the file a themer edits most. Nothing here reformats it.

`features/reaper/fts-themer/src/walter_colors.rs` retints the RGB literals
*inside* WALTER without parsing it, deliberately: only assignments whose
variable name contains `color`, only trailing 3-tuples, because `[x y w h …]`
and `[r g b]` are syntactically identical and rewriting brackets blindly would
silently relayout the theme.

### How hand-maintained is the fork, really?

Four commits have ever touched `rtconfig.txt` (`deb41d2fe` bring in-tree,
`06892b21e` provenance, `9df0f1a47` retint colour literals, `8cfc07da7` scrims).
**None changed the layout.** The 33 `FTC MOD` / `reARKMOD` markers in the file
are FeedTheCat's and reARK's, not ours. "Hand-maintained fork" currently means
"frozen inherited code we have not modified" — which is worth saying out loud,
because it is both an argument for generating it (nobody owns it) and against
(nobody has needed to).

---

## 5. Sizing the generator honestly

### The mechanism

Sampling, not translation. There is no path from a Taffy tree to WALTER
expressions; there is a path from *measurements* of a Taffy tree to WALTER
coordinates.

1. **Render headless at a size.** `dioxus-test` (vendored,
   `libs/vendor/dioxus-test/`) drives components through `blitz-dom` — the same
   engine that renders our REAPER panels. `DocumentTester::with_window_size`
   (`libs/vendor/dioxus-test/src/document.rs:79-89`) sets the viewport.
2. **Read the computed layout.** `node.final_layout` is the Taffy `Layout`:
   parent-relative `location` + `size`. `ResolvedElement::upper_left`
   (`libs/vendor/dioxus-test/src/element.rs:104-110`) already does exactly this
   read; `pointer.rs:111` already walks the tree accumulating parent-relative
   positions into absolute ones. So step 2 is a few dozen lines, today.
3. **Map node → REAPER element id.** A `data-walter="mcp.volume"` attribute on
   each component, or a registry. New, small.
4. **Find the discontinuities.** Sweep height (and width), watch each element's
   rect. Where it appears/disappears, or its slope w.r.t. the container changes,
   bisect to the exact px. These become `h<T [0]` / `h<T A B` conditionals.
5. **Fit anchors between discontinuities.** Two samples per segment give
   `ls ts rs bs` exactly *if* the response is linear; a third sample verifies it
   and rejects the segment if not.
6. **Print WALTER**, then **verify with the interpreter we already have** —
   `daw_theme_reaper::walter::evaluate` the emitted file at a dense size sweep
   and diff against the sampled Taffy rects. This is a real oracle and it
   materially de-risks the job; without it I would not recommend attempting this
   at any budget.

### What is hard, and where it breaks

- **The state cross-product, which is the actual cost.** The MCP branches on
  `stripMode` / `narrowMode` / `wideMode` / `sidebarMode`, `labelsMode`,
  `meterExpMode`, `gapmode` (5 values, `2099-2103`), `trackpanmode` (3/5/6),
  `folderstate` × `folderdepth`, `recarm`, `track_selected`, `trackcolor_valid`,
  `tracknch`, three DPI tiers, and three parameter sets A/B/C
  (`2598-2632`). Every combination must be a configuration our Dioxus panel can
  actually render, or it cannot be sampled. And the emitted program is a
  decision tree over those same states — so generation does not *shrink* the
  WALTER, it only changes who types it.
- **Between-sample drift is unbounded.** REAPER re-runs the program at every
  pixel, not at our sample points. A wrap or clamp we did not bisect becomes a
  visible jump. Discontinuity detection has to be exhaustive per element per
  axis per state, which is where the sampling budget explodes.
- **Fonts.** `*.font` is an index into REAPER's own table with per-OS mapping
  (`rtconfig.txt:404-421`, `calcFontSizes` `423-490`) and `*.margin` carries the
  justification scalar. Taffy's text metrics are not REAPER's. A generated
  layout will have the right boxes and the wrong glyph positions, and there is
  nothing to sample this from.
- **Elements we never draw.** `mcp.extmixer`, `mcp.fxlist`, `mcp.sendlist`,
  `mcp.fxparm` are REAPER-drawn lists whose row height, line spacing and
  scrollbar size are packed into `*.font` (`2221-2231`). No component of ours
  measures those. They stay hand-written regardless.
- **The colour half does not sample at all.** ~40 of the MCP's 106 assignments
  are `.color`, computed from track colour through tint/dim/select and a
  luminance test. Those must be emitted from the same Rust colour code as
  literal WALTER expressions, or kept by hand.
- **`custom` declarations, action ids and image names** (74 of them) come from
  nowhere in a Taffy tree. So do the 117 `Layout` blocks and the 101
  `layout_dpi_translate` lines.
- **All-or-nothing.** There is no partial mode. A generated `rtconfig.txt` that
  covers the MCP and leaves the TCP hand-written is fine (they are separate
  namespaces), but a generated MCP that is 95 % right is a broken mixer.

### Effort

| | scope | estimate |
|---|---|---|
| **A — mirror** | encode ~12 MCP + ~6 TCP thresholds as container queries; structure the tree so the derived residuals are containers; generate only the `hide_*` constants into rtconfig | **1–2 sessions** |
| **B — generate** | element-id registry, headless sampler, state matrix, bisecting discontinuity detector, anchor fitter, WALTER printer, verifier loop, colour-expression emitter, font mapping, hand-kept `custom`/`Layout`/DPI scaffolding | **several weeks**, and it must reach ~100 % on the MCP before it is usable at all |

Call it an order of magnitude, with B's risk concentrated in the state
cross-product and in between-sample drift — the two places where "it looked
right in the sampler" and "it looks right in REAPER" come apart.

---

## What this means for each ticket

### #128 — the breakpoints

Reproduce them. Container queries at REAPER's own thresholds (260 / 320 / 350 /
400, plus the `stretch_sec` ones at 106 / 125 / 141 / 144 / 162 and the strip's
`pan_area` 20 / 45 / 75 / 90), because #124 says REAPER is the source of truth
while mapping and these numbers are already written down. Two caveats carried
from #124's hard-won rule: these are **structure**, so read them as "a
threshold exists here, and this collapses at it" — the rendered px still get
measured off a screenshot across several columns. And several of them are not
container queries at all until the tree is structured so that `stretch_sec` and
`pan_area` are real boxes; that decomposition is the actual work and it is
worth doing on its own merits.

### #130 — the file and the panels

`rtconfig.txt` stays a hand-maintained fork, with one narrow generated seam (the
threshold constants). Full generation is deferred, not refused — and the
condition for revisiting is specific: **when a design decision is made that
REAPER's element vocabulary can still express but whose geometry we no longer
want to hand-solve.** If the decision is one REAPER cannot express at all, the
generator would not have helped.

`features/daw-ui/daw-ui/src/panels/` is **kept**, and the framing "or kept as a
viewer for other people's themes" undersells what is there: it is already that
viewer, wired and shipping in `fts-themer-ui`, and it is the only thing in the
tree that executes WALTER. What should go is the *bitmap art path* inside the
panels once the vector components supersede it — `native.rs` (109 lines) is
already the seam for that. What must stay is `mcp_strip.rs` + `LayoutEngine` +
`daw-theme-reaper`, because:

- `fts-themer-ui`'s live preview depends on it today;
- it is the verifier any future generator needs (step 6 above);
- it is how we read third-party themes at all — the corpus of four real themes
  is the only evidence we have about what WALTER programs look like in the wild.

Deleting the panels would mean deleting a 938-line corpus-tested interpreter to
avoid maintaining it, and then writing an emitter for the same language.

---

## The strongest counter-argument

**Mirroring is the thing #124 forbids, and this is the cheapest moment
generation will ever be.**

The standing decision reads: *"The REAPER theme is generated from the Dioxus
components, never authored separately."* Hand-maintaining `rtconfig.txt` is
authoring it separately, whatever we call it. And every hour spent teaching
Dioxus REAPER's thresholds is an hour spent making the FTS UI an impersonation
of REAPER — which #124 explicitly wants only *until* the map is done, after
which design decisions start. Building the generator later means building it
against a Dioxus layout that has already diverged, so the question "can WALTER
express this?" gets asked repeatedly, late, and answered no expensively. Right
now the two layouts agree, which is precisely when a sampler is easiest to
validate: the oracle for "did the generator get it right" is the shipped
`rtconfig.txt` itself, diffed through an interpreter we already own. That oracle
disappears the moment we change anything.

There is also a real, already-measured failure mode in the recommendation:
"hand-maintained" has meant "untouched" for four commits. Nobody in this repo
has ever written WALTER layout. Choosing A is choosing to keep depending on
inherited FeedTheCat code that no one here understands, and to keep the
threshold numbers duplicated in two languages with only a convention holding
them together.

**Why I still recommend A.** The counter-argument is right that generation is
cheapest now and right that A duplicates knowledge. It is wrong about the
payoff, because of the one-way half of the gap in §3: a generator can only emit
layouts drawn from REAPER's fixed element vocabulary with no content-driven
sizing. Its capability ends exactly where our design freedom begins. So the
window in which generation is *possible* is the window in which the two layouts
agree — and in that window the generated output is a file we already have. Spend
the weeks the first time a real design decision needs geometry we do not want to
hand-solve, with the sampler's requirements then known concretely rather than
guessed. Meanwhile, generating just the threshold constants removes the specific
duplication the counter-argument is right about, for about a day's work.

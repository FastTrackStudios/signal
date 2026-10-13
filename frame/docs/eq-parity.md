# EQ graph parity: Dioxus `EqGraph` → frame `viz.eq_graph`

This is the checklist of everything the Dioxus EQ graph does. The Dioxus graph lives in
`fts/processor/features/fx/eq/eq-ui/src/` (`eq_graph.rs`, `eq_graph_popup.rs`,
`eq_graph_interaction.rs`, `eq_graph_model.rs`, `eq_graph_painter.rs`, `eq_glow.rs`,
`dynamics.rs`, `cheatsheet.rs`), and signal mounts it in
`fts/signal/features/rigs/guitar/ui/src/eq_vello.rs`. Each item is checked against frame's
`viz.eq_graph`.

**Tags**

- **[frame-general]**: needs, or is built on, a reusable frame feature that any surface can use
  (an ADSR, a keys rig, a mixer).
- **[eq-specific]**: an EQ design choice. It is built from the general features, in
  `surface-kit/src/viz/eq_graph.rs`.

**Status**

- ✅ done
- 🟡 partial (the reason is given)
- ❌ not done (the reason is given)

Runtime tests are in `features/surface/surface-kit/tests/eq_graph.rs` (EQ) and
`features/surface/surface-proto/tests/interact.rs` (validation). They drive the runtime the way a
user would.

**Screenshots** (headless player: `frame examples/surfaces/10-eq-graph.frame --script <name> --screenshot out.png`):

- `select-handle`: the band panel and readout chip on a band.
- `add-on-canvas`: a band added by double-click.
- `rename-handle`: renaming in place.
- `marquee`: a marquee mid-drag.
- `context-handle`: the band menu.

## General features added to frame

| Feature | Where | API |
|---|---|---|
| Dynamic collections | `surface_proto::ControlKind::Repeat`, `surface_runtime::runtime::repeat` | `Repeat { first, count, used }`. One template subtree is drawn per item of an indexed family. `{n}` in addresses, names and text resolves per item, and instance ids are stable (uuid v5). An item shows while `used` is on. Adding an item sets `used` (after resetting the item's params to their defaults), and removing one clears it. `Runtime::add_at`, `remove_items`, `instances`, `item_of`, `item_used` |
| Context subtrees | `Popover { context }` / `Menu { context }` | One popover or menu serves every item and is rebound to the active item: the dragged one, else the hovered one, else the last selected. |
| Text values | `ParamKind::Text`, `TextValue`, `SurfaceEngine::{texts, set_text}` (default: none / `NotText`), `EngineCall::SetText`, `sync::Engine::texts` | Supported by the mock engine (`Memory::text`) and over vox. `Runtime::{texts, apply_texts, text}` |
| Text field control | `ControlKind::TextField { click_to_edit, max_len }` and slots `Value` / `Placeholder` / `Caret` / `TextSelection` | Supports click-to-caret, Shift-extend, typing (key text and IME commits via `Runtime::type_text`), Backspace/Delete, arrows, Home/End, Cmd/Ctrl-A, Enter/Tab to commit and Escape to cancel. `Runtime::{begin_edit, commit_edit, cancel_edit, edit_view, set_text_measure}`, `render::text_advance`. There is also a canonical `surface_widgets::Builder::text_field`, and generated surfaces give text params one. |
| Keyboard focus routing | `Runtime::key` → `Option<calls>` | Keys go to the field being edited, then an open menu, then the selection (Delete/Backspace remove, Escape clears, letter key rules), then the focused control. `None` means the host may use the key (the player quits on an untaken Escape). |
| Anchored popovers | `ControlKind::Popover { anchor, placement, gap, show, context }`, `Placement`, `Show` | The popover follows its anchor, flips to the other side when it doesn't fit its stage, is clamped inside the stage, and is lifted above everything. `Show` is Always / Active / Selected / Hovered. Hit testing is done where it is drawn. `Runtime::{popover_origin, floating_origin}` |
| Context menus | `ControlKind::Menu { items, context }`, `MenuItem { row, role, commands, checked, check, submenu }`, `Interact::menu` | Opened by right-click at the pointer + 10 px and clamped. Rows are highlighted on hover (`Highlight` slot). Check marks follow a `Condition`. Submenus open beside the row and flip left when there is no room. The keyboard moves with Up/Down, opens and closes submenus with Right/Left, closes with Escape, and runs with Enter. A press outside closes the menu. `Runtime::{context_click, open_menus}` |
| Commands | `Command::{Set, Toggle, Cycle, Invert, Reset, Invoke, Remove, Edit, Add}` | Shared by menus, chord clicks, double-click, press and key rules. They resolve `{n}` against the item. |
| Selection model | `Interact { selectable, removable, click, … }`, `ClickAction::{Select, ToggleSelection, RangeSelect, Run}`, slots `Selected` / `Hovered` | Supports marquee (`Canvas` + `Marquee` slot; Shift adds), group drag (every selected handle moves by the same unit travel), drag-out-to-remove (the handle fades while out), and Delete. `Runtime::{selection, select, clear_selection, hovered}` |
| Canvas gestures | `ControlKind::Canvas { x, y, add: AddRule }`, `AddZone`, `Assign` | A double-click on empty space adds an item at the click, mapped through the axes and the zoom. The first matching zone sets the rest (a shape inferred from position). The new item is selected and captured, so it drags straight away. A click clears the selection. Right-click opens its menu (`Command::Add` there). |
| Modifier-aware handles | `Modifiers::option`, `Chord`, `DragRule { chord, mode, when }`, `DragMode::{Free, XOnly, YOnly, Wheel{per_px}}`, `WheelRule { chord, target, step, when }`, `KeyRule` | Chord clicks are deferred to a release without travel. Cmd/Ctrl becomes a chord rather than a reset when a control has rules. |
| Conditions, zoom, tint, on/off | `Condition`, `Control::{show_if, zoom, tint}`, slots `On` / `Off` / `Tint`, `ButtonMode::Cycle` | `show_if` hides the node and stops hits on it. `Zoom` sets the y range from a choice param. `Tint` is a colour ramp at a handle's x, a curve's `tint` input or a control's position. |
| Curves | `eq.response` | Adds 10 shapes (Tilt Shelf, Flat Tilt, All Pass), `b{n}_used`, `b{n}_slope_index` (the wire's 0‥10 slope), `stems` → `b{n}_stem`, and `b{n}_dyn_range` → `b{n}_dyn` |
| Lint and library | `design-lint`, `library_proto::Signature` | Templated controls are validated as item 1, and new kinds get their slot rules. Floating controls are skipped for overlap. The signature covers every item of a repeat, plus rule, command and zoom params. |
| Headless scripts | `frame --script select-handle \| add-on-canvas \| rename-handle \| marquee \| context-handle` | Scripted input goes through the window mapping. The report JSON includes the selection, the open menus and the editing text. |

## 1. Data model

- ✅ Up to 24 bands, `b{n}_used` [frame-general: `Repeat` + `used`]. The first free slot is taken on add, and nothing happens when all 24 are used.
- ✅ Per band: `used`, `on` (bypass), `freq` (10–30 k Hz), `gain` (±30 dB), `q` (0.025–40), `shape` (10), `slope` (index 0–10), `name`, `placement` (ST/L/R/M/S), `focus`, `dyn_range` (±30) [eq-specific catalog: `viz::catalog::eq_params`]. These match signal's wire names and ranges.
- ✅ All 10 shapes, in signal's wire order: Bell, Low Shelf, High Shelf, Low Cut, High Cut, Notch, Band Pass, Tilt Shelf, Flat Tilt, All Pass [eq-specific: `eq.response`].
- 🟡 Tilt Shelf and Flat Tilt are drawn as shelf pairs (±g/2). This approximates eq-dsp's exact designs.
- ✅ Slope table: 0/6/12/18/24/30/36/48/72/96/Brickwall (`SLOPES`, `curves::slope_db`). Brickwall is drawn as 96 dB/oct.
- ✅ Names are empty by default, and the label only shows when a name is set [frame-general: an empty text hides its `Value`].
- ✅ Band colour comes from its frequency: HSL(t·290°, 68 %, 58 %), low frequencies red, high violet [frame-general `tint` ramp, eq-specific ramp].
- 🟡 Dynamics: only the per-band `dyn_range` exists. There is no dynamic/spectral mode, threshold, attack/release, auto, tilt or density params, and no DynState UI. Those params aren't on signal's graph wire today, and signal doesn't pass `band_dynamics`.
- ❌ Stereo power-sum across L/R/M/S in the combined curve. `eq.response` sums dB, and placement doesn't enter the curve. That needs a stereo-aware provider.

## 2. Mapping

- ✅ Log frequency axis, 20 Hz–20 kHz by default (`f_min=`/`f_max=`). Dioxus defaults to 24 kHz.
- ✅ Symmetric dB axis, with nodes pinned at the edges.
- ✅ Range steps ±3/6/12/18/24/30 [frame-general: `Zoom` on a choice param]. The default is ±12, as signal uses.
- ✅ Range selector: a cycle button, where a double-click resets it [frame-general: `ButtonMode::Cycle`, `Interact::double_click`].
- 🟡 The Dioxus selector is a `<select>` dropdown; frame's range control is a cycle button. A dropdown could be built from a `Menu`.
- ✅ The dB numerals follow the range [readouts with `ValueFormat::labels`].
- ❌ Auto-range (expanding the range while dragging near the edge). The gesture's unit-space accumulator would jump when the zoom changes mid-drag; this needs a zoom-aware gesture rebase.

## 3. Adding bands

- ✅ Double-click on empty area adds a band [frame-general: `Canvas` + `AddRule`]. Frame uses the host's double-click detection (350 ms / 4 px). Dioxus uses 400 ms / 20 px.
- ✅ Frequency and gain come from the click position, clamped.
- ✅ The shape is inferred from the position, in order [eq-specific `AddZone`s]:
  1. gain < −82 % of the range → Notch
  2. < 30 Hz and near 0 dB → Low Cut
  3. > 15 kHz and near 0 dB → High Cut
  4. < 80 Hz → Low Shelf
  5. > 8 kHz → High Shelf
  6. otherwise → Bell
- ✅ Shapes without gain get 0 dB. New bands start at Q 1.0, on. Cuts get Q 0.707 and keep the default slope of 12.
- ✅ The new band can be dragged at once, since the add happens on the press.
- ✅ The new band is selected and its panel comes up. Dioxus only hovers it; this exceeds Dioxus.
- ✅ The params of a reused slot are reset to their defaults, including the name.
- ✅ Empty-area right-click menu with "Add Band Here".
- 🟡 The Dioxus row reads "Add {Shape}"; frame's shape comes from the zones when you click. A per-point label needs a readout of the would-be shape.
- ✅ "No free band slots" behaviour: nothing is added.
- 🟡 Frame shows no disabled row for this.
- ❌ Alt / Alt+Shift double-click to create a dynamic / spectral band. There are no dynamics modes (see §1).

## 4. Band interactions

- ✅ Hit testing uses a 30 px touch box. Dioxus uses a 15 px radius.
- ✅ Chord clicks [frame-general `ClickRule`]. Frame maps them the same way Dioxus does:
  - Alt+Cmd: cycle the shape.
  - Alt+Shift: cycle the slope.
  - Alt: bypass.
  - Cmd or Cmd+Shift: toggle membership in the selection.
  - Shift: range-select by band index.
  - None: select.
- ✅ Chord actions are deferred to a release without travel (3 px), so an Alt-drag doesn't bypass.
- ✅ A plain press on a selected band keeps the group, so the group drags together.
- ✅ Double-clicking a band renames it and doesn't reset it [frame-general: `double_click: [Edit]`].
- ✅ Drag modes [frame-general `DragRule`]:
  - Alt drags frequency only.
  - Cmd is resonance: vertical travel sets Q, doubling per 60 px.
  - Shapes without gain slide across only, by default.
- ✅ Shift is fine. This exceeds Dioxus, whose free drag is absolute.
- ✅ Group drag: the same frequency ratio and the same gain travel. Dioxus scales the gain proportionally; frame moves every band's gain by the same amount.
- ✅ Dragging a band out of the graph and letting go deletes it. The band fades while it is out, and dragging it back in keeps it [frame-general `removable`].
- 🟡 Dioxus also ends a drag when a move arrives without the button held; frame's host sends `Cancel` or `Up`.
- ✅ Wheel targets [frame-general `WheelRule`]:
  - Wheel alone: Q (×1.15 per notch), or the slope for a cut.
  - Cmd: gain, 1 dB a notch.
  - Alt: dynamic range, 1 dB a notch.
  - Alt+Cmd: gain.
  - Shift: fine.
- 🟡 Alt+Cmd (GainAndRange in Dioxus) turns gain only.
- ✅ Hover [frame-general `Hovered` slot]: a brighter halo, and the panel comes up.
- ✅ Marquee: dashed blue rect with a translucent fill. It replaces the selection, and Shift adds (exceeds Dioxus).
- ✅ A click on empty space clears the selection, but not with Shift.
- ✅ Delete/Backspace removes the selected bands. Dioxus's plugin bypasses them; frame removes them, as signal does.
- ✅ Escape clears the selection.
- ✅ Placement keys m/s/l/r/n and focus f act on the selection [frame-general `KeyRule`].
- ❌ The `d` delta toggle. There is no global delta param on the graph wire.
- ❌ `b` / Shift+B sweep. This needs key-up events and a held-state restore, and the runtime has no key-up input yet.
- ❌ A cursor change. Dioxus has none either.

## 5. Band panel (Dioxus "popup")

- ✅ The panel shows for the active band: dragging, hovered or selected [frame-general context `Popover`, `Show::Active`]. It stays up while the pointer is on it.
- ✅ The panel follows its node on both axes, under it, and flips above when there is no room. It is clamped inside the surface. This exceeds Dioxus, whose panel is docked to the bottom.
- ✅ Look: #14141a with a #2a2a30 rim, 10 px radius and a drop shadow. The top edge is a 2 px bar in the band colour, tinted by the band frequency.
- ✅ Bypass button (●), lit while on.
- ✅ Shape button: a click cycles the shape.
- ✅ Stereo placement button, cycling ST/L/R/M/S.
- ✅ Focus toggle.
- ✅ Delete button (×) [frame-general: a `Button` with `press: [Remove]`].
- 🟡 The focus glyph is "F"; Inter has no ∩.
- ✅ A name field in the panel (exceeds Dioxus, which has none): "Name…" placeholder, live edits, caret and selection.
- ✅ FREQ / GAIN / Q / DYN knobs (`knob.soft_pointer`). Dioxus shows these only when handles are passed; signal gets text readouts.
- ✅ Knob visibility follows the shape: a cut shows SLOPE instead of Q, and shapes without gain hide GAIN and DYN [frame-general `show_if`].
- ❌ The dynamics row (Static/Dynamic/Spectral modes, ± range, live value). There are no dynamics modes (see §1).
- ❌ Pinning the panel's x for 2 s after touching it. Frame's panel follows the band; it stays up while hovered.
- ✅ Readout chip above the band: frequency and gain on one line, Q and shape below. Shapes without gain hide the gain.
- 🟡 The Dioxus chip reads "N dB/oct" for cuts on its second line; frame's chip shows Q there.

## 6. Context menus

- ✅ Band menu on right-click, at the pointer + 10, clamped. It selects the band [frame-general `Menu` + `Interact::menu`].
- ✅ Look: #121216 at 98 % with a rim, 6 px radius and a shadow. Rows are 20 px with a hover highlight rgba(120,150,255,.16), and the danger row is red.
- ✅ Items:
  - Header "BAND n · freq"
  - Bypass ✓, Focus ✓
  - Invert Gain
  - Shape › (10 shapes, ✓). Cut shapes set Q 0.707; shapes without gain set 0 dB.
  - Slope › (11, ✓)
  - Stereo Placement › (5, ✓)
  - Reset Gain
  - Rename (exceeds Dioxus)
  - Delete Band
- ✅ Submenus open beside their row and flip left when there is no room. Keyboard navigation works (exceeds Dioxus).
- 🟡 The Bypass label doesn't flip to "Enable"; the ✓ shows the state instead.
- 🟡 The placement submenu order is Stereo/Left/Right/Mid/Side, not Dioxus's Left/Right/Stereo/Mid/Side, and it has no coloured dots.
- ❌ Make Dynamic / Make Spectral. There are no dynamics modes.
- ✅ The graph menu is on empty space: header "GRAPH" and "Add Band Here".
- 🟡 The Dioxus header reads "Graph · freq / gain".

## 7. Labels on nodes

- ✅ The name label rides above the node, 10 px, weight 600, with a black shadow. It flips below at the top edge [frame-general `Popover { show: Always }`]. The label isn't tinted: it is #e9ecef, where Dioxus uses the band colour.
- ✅ Inline rename at the label: it starts empty (typing replaces), the caret and selection are drawn, Enter commits and Escape restores [frame-general `TextField { click_to_edit: false }` + `Command::Edit`].
- 🟡 Dioxus shows the old name as grey held text while the field is empty. Frame's field starts with the old name selected, so typing replaces it.
- ✅ No band-number badges, as in Dioxus.

## 8. Painting

- ✅ Dark ground. The grid has decade and major tiers, and the zero line is major.
- 🟡 The dB grid is at eighths of the range. Dioxus uses 2 / 3 / 6 dB by range.
- ✅ Frequency labels 20…20k along the bottom, the first left-aligned and the last right-aligned. dB labels on the right follow the range.
- ✅ Analyser layers [eq-specific `Plot`s + frame-general `show_if`], each toggled by PRE / POST / EXT buttons:
  - pre: grey fill and line
  - post: blue fill and line
  - external / sidechain: orange line
- ❌ Collision shading. It needs a per-bin comparison of two feeds. A curve provider only takes scalars, so it needs feed-array curve inputs (or the engine publishing a collision feed).
- ✅ Per-band curves: filled to 0 dB and stroked in the band colour. Bypassed and unused bands aren't drawn [frame-general `Repeat` + `tint`].
- ✅ Stems from the node to 0 dB when |response| > 0.1 dB (`eq.response` `stems`).
- 🟡 Dynamic range: a dashed curve at gain + range. Dioxus fills a ribbon between the two curves and draws a live curve; frame has no between-curves area style and no live-gain feed.
- ✅ Combined curve in gold (#d4a932, 2 px) with a soft under-stroke and a faint fill to 0 dB.
- ❌ Model response curve. There is no model feed.
- ✅ Nodes: a 14 px disc tinted by frequency with a white rim, and a resting halo. Hover brightens the halo, and a selected node has a ring in the band colour. A bypassed node is grey #555 with no curve.
- 🟡 Dioxus grows the node when hovered or dragged (r 7→9→10); frame changes the halo only.
- 🟡 Glow: frame's glow is emission-free, because light composites over the floating panels. A `glow` frame at the bottom of the traces is left as the hook for the GPU glow component, which is being ported separately; the analyser field and node halos come with it.
- ❌ Cheat-sheet instrument zones and their profile selector. This is data (profiles and zones) plus a profile choice. It is buildable with `show_if` rects per profile, but not done; signal passes `show_hints = false`.
- ❌ Ear-cue ruler and too-much / too-little hints. These are `show_hints` extras, off in signal.

## 9. Signal integration

- ✅ The catalog mirrors signal's wire: `b{n}_{used,on,freq,gain,q,shape,slope}` with the same ranges and encodings: shape index in `all()` order, slope index 0–10.
- ✅ Adding a band writes its params, then `used = 1`.
- ✅ Removing a band writes `used = 0`.
- ✅ Names travel as a text param: `set_text` on the wire (with a backward-compatible default). Signal never sends names today.
- ✅ The mock engine keeps names (`Memory::text`), and the desktop player shows them live. A reload keeps them.
- ❌ Text changes don't stream from other surfaces. There is no text subscription, because adding one would break existing `SurfaceEngine` stream implementors; texts are read on connect and reload.

## 10. Other

- ✅ Markup round-trip: the authored graph (templates, popovers, menus, rules) prints and parses to the same document, and it lints clean (`the_graph_lints_clean_and_round_trips_through_markup`).
- ✅ The document stays small: one template, not 24 copies. Instances exist only in the runtime.
- ❌ Undo. Neither Dioxus nor frame's runtime undoes engine edits.
- ❌ Touch and pinch. Not in Dioxus either.

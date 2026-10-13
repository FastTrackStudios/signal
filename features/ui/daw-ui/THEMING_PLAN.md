# Themeable DAW UI — build plan

Goal: UI components themeable to REAPER's capacity, **vector-first** (the thing
REAPER gets wrong — it forces images), with an **optional image-skin** layer,
and ultimately a **REAPER-theme importer**. Modeled on WALTER's separation of
concerns (see `../../../reaper-theme/docs/`). Checklist — tick as we go.

---

## Phase 1 — Theming foundation (the token spine) ← start here

- [ ] **Role taxonomy** — `enum ElementRole` for every themeable piece
      (panel surfaces, strip, tcp-row, fader, meter, knob, mute/solo/arm/
      monitor/phase/fx/route buttons, label, trackidx, lane, clip, ruler,
      folder header, master variants). The stable id a theme targets + a
      converter maps onto. Mirrors WALTER's element vocabulary.
- [ ] **`ThemeState`** — extend `ControlState` (idle/hover/drag/disabled) with
      `selected / armed / soloed / muted / recmon / folder_depth / track_color`.
      Mirrors WALTER scalars (`recarm`, `folderstate`, `trackcolor`).
- [ ] **`Theme` struct (owned, cloneable)** — palette → **semantic roles**
      (`surface`, `surface_raised`, `accent`, `text`, `text_dim`, `border`,
      `meter_safe/warn/danger`, `track_tint`, …). Replaces the `&'static dyn
      StyleSheet`; presets become `Theme` constructors.
- [ ] **Token derivation helpers** — `mix / tint / dim / readable_on /
      with_alpha` (lift the ad-hoc ones out of `mixer.rs`); `track_tint`
      computed from surface + track color + params.
- [ ] **Per-element style structs** — keep `KnobStyle/SliderStyle/XYPadStyle`;
      add `PanelStyle, StripStyle, TrackRowStyle, MeterStyle, FaderStyle,
      ButtonStyle, LaneStyle, ClipStyle, RulerStyle, FolderStyle`. Each
      resolved by `(role, ThemeState)`.
- [ ] **`Theme::style_for(role, state)`** API + `use_theme()` update; provide
      a default **dark** preset.
- [ ] **Migrate the new panels off their hardcoded palette** onto the theme
      (immediate visible payoff). 

## Phase 2 — Theme parameters (the `define_parameter` analog, improved)

- [ ] **`ThemeParams`** — typed params (`name/desc/default/min/max/kind`,
      grouped) that *derive* tokens (tint strength, dim-when-selected, accent,
      density, meter style, corner radius, border on/off…).
- [ ] **Live theme adjuster** panel in the showcase — drag params, watch tokens
      update instantly (REAPER needs a relayout; we don't).
- [ ] **Theme switcher** — cycle presets at runtime.

## Phase 3 — Track views (one model, three representations)

The same `TrackView` rendered three ways — *"what a track looks like"* per
context. All vector, all theme-driven, all sharing the per-track `Signal`s.

- [ ] **Track in TCP (`TcpRow`)** — horizontal control row: arm · color stripe ·
      name (folder-indented) · horizontal volume fader · pan knob · mute/solo ·
      compact meter. Variable height (REAPER `tcp_heights`: supercollapsed /
      collapsed / small / recarm). Later: fx, io, recmon, recinput.
- [ ] **Track in MCP (`ChannelStrip`)** — vertical strip: pan knob · solo/mute ·
      fader+integrated meter · routing · name footer. *(exists — re-theme.)*
- [ ] **Track in Arrange (`Lane`)** — timeline lane with clips, height aligned
      to the TCP row. *(exists — re-theme + grid/selection.)*
- [ ] **Folder representation** in each context — TCP indent + folder header,
      MCP group header, Arrange folder lane. *(partial — unify on depth model.)*
- [ ] **Master track** variants — master TCP row + master MCP strip
      (mono button, menu button, master meter w/ RMS).
- [ ] **Selection / arm / solo / mute visuals** wired through `ThemeState`.

## Phase 4 — Shared themeable widgets (vector default + image skin)

- [ ] **Meter** — promote inline meter to a real widget: mono/stereo, peak
      hold, RMS, safe/warn/danger zones, dB scale, clip indicator. Themeable.
- [ ] **Fader** (vertical + horizontal) with integrated meter — generalize
      `ChannelFader`; theme via `FaderStyle`.
- [ ] **Knob** — pan/param; already themed, fold into the new token API.
- [ ] **Button family** — `Mute, Solo, RecArm, Monitor, Phase, Fx, Route`
      as one themeable toggle/icon set (replace ad-hoc mixer buttons).
- [ ] **Clip/item** — name, color, fades, selection; `ClipStyle`.
- [ ] **Ruler / timebase** — beats/time, grid lines; `RulerStyle`.
- [ ] **Label / trackidx / io / recinput** small elements.
- [ ] **Optional image-skin hook** — a `Skin` trait: vector by default, image
      override per role (3-slice button / knob filmstrip / meter slices /
      9-slice bg). The bridge to REAPER-theme image atlases.

## Phase 5 — Layout & interaction

- [ ] **Anchor-box descriptor** (`[x y w h]` + edge-attach scalars) + role-id on
      components — native layout uses flex, but this is the **converter target**
      and enables data-driven/swappable layouts.
- [ ] **Resizable splitters** — TCP width, mixer height, lane heights (drag).
- [ ] **Named layouts + scale/DPI variants** (REAPER `Layout` + `misc_dpi_translate`).
- [ ] **Density / compact modes** for the TCP row heights.
- [ ] **Ruler↔lanes scroll sync**; horizontal timeline scroll polish.

## Phase 6 — Serialization, hot-reload, presets

- [ ] **Serializable `Theme` + `ThemeParams`** (RON/JSON).
- [ ] **Hot-reload** a theme file in the showcase (edit → live update).
- [ ] A couple of **first-class presets** (FTS dark default + one accent变体).

## Phase 7 — REAPER theme importer (the payoff)

- [ ] **Colors/params first** — parse `.ReaperTheme` palette + `rtconfig`
      globals + `define_parameter` → our tokens/params (works for solid themes).
- [ ] **Layout** — WALTER `Layout`/`set`/conditionals → anchor-box tree.
- [ ] **Images** — slice atlases (3-slice / filmstrip / meter / 9-slice) → the
      image-skin layer.
- [ ] **Element-id map** — REAPER ids (`tcp.volume`, `mcp.mute`, …) → our roles.

---

### Recommended order
Phase 1 → 3(TCP/MCP/Lane re-theme) → 2(adjuster) → 4(widgets) → 5 → 6 → 7.
Phase 1 forces the token taxonomy everything else hangs off.

# frame markup: authoring reference

frame markup is a JSX-like text form of a frame design. Each element is one node, and child elements are nested in paint order: the first child is painted first, so it sits at the back. Write the whole tree in one go.

## Syntax

```
<Tag attr=value attr=value>children</Tag>      <Tag attr=value />
```

- **Values** come in four forms:
  - `"string"` uses JSON string escapes (`\"`, `\\`, `\n`).
  - Bare numbers: `12`, `-4.5`, `1e3`.
  - `true` / `false`.
  - `{json}`: any JSON value in braces, e.g. `fill={["#ff0000"]}`.
- A bare attribute name means `true`, so `clip` is the same as `clip=true`.
- Comments are written `<!-- … -->`.
- Only `<Text>` may contain text. Write `&lt;`, `&amp;` and `&lbrace;` for `<`, `&` and `{`.
  - If the body is on one line, it is kept verbatim.
  - If the body spans several lines, each line is trimmed and the lines are joined with single spaces (JSX rules).
  - For real line breaks, use `text="line 1\nline 2"`.
- **Colors** are `"#rrggbb"`, `"#rrggbbaa"`, `"#rgb"` or `"#rgba"` (straight alpha).
- **Units** are px. Angles are in degrees.
- **Coordinates:** `x`/`y` are relative to the parent's top-left, and y points down.

A document has exactly one root, `<Document>`, whose children are `<Page>`s. A fragment is one or more elements without `<Document>`/`<Page>`, and is what you send to insert into an existing page or frame.

## Elements

| Element | Use |
|---|---|
| `Document` | Root. Children are pages. |
| `Page` | A canvas. Children are top-level frames. |
| `Frame` | Container: background, clipping, auto layout. The main building block. |
| `Group` | Plain grouping, no own paint. |
| `Rectangle`, `Ellipse`, `Line`, `Polygon`, `Star` | Shapes sized by `w`/`h`. A `Line` is drawn by its stroke along its width. |
| `Vector` | Arbitrary outline given by `geometry` (SVG path data). |
| `BooleanOperation` | Combines its children's shapes. `op="union\|subtract\|intersect\|exclude"`. |
| `Text` | Text. The content goes in the body. |
| `Component`, `ComponentSet` | Reusable main component, or a set of variants. |
| `Instance` | A use of a component. `component="<component id>"`. |
| `Utility` | Non-rendering helper (slice, widget). |
| `Other` | Unknown imported type. `tag="…"` is required. |

## Attributes (all elements)

Every attribute is optional; if you leave one out, it takes the default shown.

| Attribute | Value | Default |
|---|---|---|
| `name` | string | the element name (`"Frame"`, `"Text"`, …) |
| `id` | UUID string. Leave it out on new elements; one is generated. Keep existing ids when editing. | new |
| `visible`, `locked` | bool | `true`, `false` |
| `x`, `y` | position in the parent | `0` |
| `rotation` | degrees, clockwise, about the node's top-left | `0` |
| `transform` | `{{"a":…,"b":…,"c":…,"d":…,"tx":…,"ty":…}}`: full 2×3 matrix, `x' = a·x + b·y + tx`, `y' = c·x + d·y + ty`. Missing keys come from the identity. Use it instead of `rotation`, not together with it. `x`/`y` override `tx`/`ty`. | identity |
| `w`, `h` | size | `0` |
| `opacity` | 0–1 | `1` |
| `blend` | `normal pass-through multiply screen overlay darken lighten color-dodge color-burn hard-light soft-light difference exclusion hue saturation color luminosity linear-burn linear-dodge` | `normal` |
| `radius` | `8`, or `"tl tr br bl"` | `0` |
| `clip` | bool: clip children to the bounds | `false` |
| `mask` | `alpha`, `vector` or `luminance`: this node masks the siblings painted after it | none |
| `fill` | a color, or `{[paint, …]}` (see Paints). `"none"` means no fill. | no fill |
| `stroke` | same forms as `fill` | no stroke |
| `stroke-weight` | number | `1` |
| `stroke-align` | `inside`, `center` or `outside` | `center` |
| `stroke-cap` | `none round square line-arrow triangle-arrow diamond-filled circle-filled triangle-filled` | `none` |
| `stroke-join` | `miter`, `bevel` or `round` | `miter` |
| `stroke-miter` | number | `4` |
| `stroke-dash` | `"dash gap …"` | solid |
| `stroke-dash-offset` | number | `0` |
| `effects` | `{[effect, …]}` (see Effects) | none |
| `geometry` | SVG path data in node-local px: `"M0 0 L10 0 Z"`, or `{["…","…"]}` for several. Drawn exactly as written (it may reach outside `w`×`h`). A `stroke` strokes it; open subpaths (no `Z`) are stroked as lines and never filled. Shadows and glows follow the outline. | none |
| `stroke-geometry` | same, for pre-outlined strokes (filled with the stroke paint instead of stroking `geometry`) | none |
| `svg-snapshot` | asset id of a pre-rendered SVG | none |
| `source` | `"tool/TYPE/id"` (import provenance). Don't invent it. | none |

### Auto layout (containers)

With `layout` set, a container places its children like CSS flexbox and ignores their `x`/`y`. The other attributes in this group only work together with `layout`.

| Attribute | Value | Default |
|---|---|---|
| `layout` | `row` or `column` | none (children use `x`/`y`) |
| `gap` | space between children | `0` |
| `padding` | `12`, `"v h"`, `"t h b"` or `"t r b l"` | `0` |
| `wrap` | bool | `false` |
| `justify` | main axis: `start`, `center`, `end` or `space-between` | unset (start) |
| `align` | cross axis: `start`, `center`, `end` or `baseline` | unset (start) |

### Inside an auto-layout parent

| Attribute | Value | Default |
|---|---|---|
| `grow` | share of the free main-axis space (`1` fills it) | unset |
| `align-self` | `start`, `center`, `end` or `stretch` | unset |
| `absolute` | bool: out of flow, placed at its own `x`/`y` | `false` |
| `min-w`, `min-h`, `max-w`, `max-h` | size limits | unset |

## Text attributes (`<Text>` only)

| Attribute | Value | Default |
|---|---|---|
| body, or `text` | the characters | `""` |
| `font` | family, e.g. `"Oswald"` (see [Fonts](#fonts)) | `"Inter"` |
| `size` | font size | `14` |
| `weight` | 100–900 (400 regular, 700 bold) | regular |
| `style` | `"italic"`, or a style name such as `"Bold Italic"` or `"Condensed"` | upright |
| `stretch` | width in percent: `75` condensed, `125` extended | `100` |
| `line-height` | px | natural |
| `letter-spacing` | px | `0` |
| `case` | `original upper lower title small-caps small-caps-forced` | `original` |
| `text-align` | `left`, `center`, `right` or `justified` | `left` |
| `runs` | `{[{"text":"Bold","font_weight":700,"fills":["#ff0000"]}, …]}`: styled spans in order. Their `text` values concatenate to the characters. Span keys: `text`, `fills`, `font_family`, `font_size`, `font_weight`, `font_style`, `font_width`, `letter_spacing`. A span inherits the keys it leaves out. | none |

The text color is `fill`.

### Fonts

Every renderer bundles these families. Write `font`, `weight`, `style` and `stretch` rather than faking weight with overlapping copies or outlines:

| Family | What it is — use it for | Weights | Italic / width |
|---|---|---|---|
| `Inter` | UI sans (the default): labels, values, legends | 100–900 | italic |
| `Oswald` | condensed display sans: model names, big legends, amp badges | 200–700 | slanted |
| `Saira` | technical sans with a width axis: condensed to extended rack lettering | 100–900 | `stretch` 50–125 |
| `Michroma` | wide engraved/geometric caps: brushed-metal panels, hi-fi faceplates | 400 | slanted |
| `Orbitron` | geometric tech caps: synth and modern-metal logos | 400–900 | slanted |
| `Dancing Script` | flowing script: signature logos, boutique and vintage badges | 400–700 | — |
| `Pacifico` | fat retro brush script: surf, funk and 50s logos | 400 | — |
| `Caveat Brush` | marker handwriting: scribbled labels, tape strips | 400 | — |
| `Knewave` | heavy brush marker: punk, funk and graffiti logos | 400 | — |
| `Playfair Display` | high-contrast serif: boutique, hi-fi, valve-era plates | 400–900 | slanted |
| `DSEG7 Classic` | 7-segment LCD/LED digits: tuners, BPM, time (digits and a few letters) | 300, 400, 700 | italic |
| `DSEG14 Classic` | 14-segment alphanumeric LCD: preset and patch names | 400, 700 | italic |
| `Doto` | dot matrix: VFD and LED-matrix displays | 100–900 | — |
| `VT323` | pixel terminal: CRT readouts, retro digital units | 400 | — |
| `JetBrains Mono` | monospace: readouts that must not jitter, tables | 100–800 | slanted |
| `UnifrakturCook` | blackletter: metal and gothic logos | 700 | — |
| `Pirata One` | condensed gothic display: metal logos | 400 | — |

Generic names work too: `sans-serif`/`system-ui` (Inter), `serif` (Playfair Display), `monospace` (JetBrains Mono), `cursive` (Dancing Script), `fantasy` (Pirata One), and `DSEG7`, `DSEG14`, `lcd`, `dot-matrix`, `marker`, `blackletter`. Names ignore case, spaces and hyphens (`jetbrains-mono`). An unknown family, or a character a family lacks (DSEG has no accents), falls back to Inter. Weights a static family doesn't have are emboldened, and italics it doesn't have are slanted. A document can bring its own fonts as `font/ttf` or `font/otf` assets: their families can then be named like the bundled ones.

```jsx
<Text name="Logo" w=300 h=60 size=44 font="Pirata One" fill="#e8e2d0">Blackout</Text>
<Text name="Model" w=200 h=30 size=22 font="Oswald" weight=600 case="upper" letter-spacing=2 fill="#f2f2f0">Mk II</Text>
<Text name="Badge" w=200 h=40 size=30 font="Dancing Script" weight=700 fill="#ffd27a">Funk Works</Text>
<Text name="Rack" w=200 h=20 size=14 font="Saira" weight=700 stretch=62.5 fill="#c7c7cc">STEREO DELAY</Text>
<Text name="Tuner" w=120 h=40 size=32 font="DSEG7 Classic" style="italic" fill="#ff3b30">440.0</Text>
```

## Paints (`fill={[…]}` / `stroke={[…]}`)

The list is painted bottom to top. Each entry is one of the following:

- `"#rrggbb"`: a plain solid color.
- `{"kind":{"Solid":{"color":"#ff000080"}}, "opacity":0.5}`
- `{"kind":{"LinearGradient":{"stops":[{"position":0,"color":"#ffffff"},{"position":1,"color":"#000000"}]}}}`. `RadialGradient` and `AngularGradient` take the same form. An optional `"transform":{…}` (a 2×3 matrix in unit space) re-orients the gradient; leave it out for the default orientation.
- `{"kind":{"Image":{"asset":"<asset id>","scale_mode":"Fit"}}}`. `scale_mode` is `Fill`, `Fit`, `Crop`, `Tile` or `Stretch`. Markup names assets by id only; a `.frame` file's image and font bytes live next to it in `<file>.frame.assets/` (`<asset id>.<ext>` plus `manifest.json`), so keep an id you copy and don't invent one.

Optional paint keys:
- `"opacity"`: 0–1, default `1`.
- `"blend"`: PascalCase (for example `"Multiply"`), default `"Normal"`.
- `"visible"`: default `true`.

## Effects (`effects={[…]}`)

```
{"kind":"DropShadow","color":"#00000040","offset_x":0,"offset_y":2,"radius":4,"spread":0,"visible":true}
```

- `kind` is `DropShadow`, `InnerShadow`, `LayerBlur` or `BackgroundBlur`.
- `kind`, `color` and `radius` are required. `offset_x`, `offset_y` and `spread` default to `0`, and `visible` defaults to `true`.

## Components and instances (themes)

A `<Component>` is a control drawn once; an `<Instance of="name">` places
it. Arguments are `arg:<name>="<expr>"` — the defaults on the component,
the values on an instance — and the component's bindings read them as
`arg.<name>`:

```
<Component name="button" w=21 h=20 arg:on="false" arg:control="\"\"">
  <Rectangle w=21 h=20 bind:fill="arg.on ? $mute : (ui.hover == arg.control ? $hover : $off)" />
</Component>

<Instance name="@mute" of="button" x=30 y=60 arg:on="track.mute" arg:control="\"mute\"" />
```

`of` names the component, not its id, so a theme can swap what an
instance draws. The theme runtime replaces each instance by a copy of its
component when the theme compiles: the copy keeps the instance's name
(`@mute` stays a hit target), position, size and own `bind:*`. A page of
components alone draws nothing itself. Instances may sit inside
components; an argument's value is read where its instance is.

## Errors

Parse errors look like `line:col: message`, e.g. `3:16: unknown attribute `colour` on <Rectangle> — did you mean `fill`?`. Fix the named spot and send the markup again.

## Examples

### 1. A card (auto layout)

```jsx
<Frame name="Card" w=240 h=120 fill="#ffffff" radius=12 stroke="#e0e0e0" layout="column" gap=4 padding=16 effects={[{"kind":"DropShadow","color":"#0000001f","offset_y":2,"radius":8}]}>
  <Text name="Title" size=16 weight=600 fill="#111111">Master Bus</Text>
  <Text name="Subtitle" size=12 fill="#777777">-3.2 dB · stereo</Text>
</Frame>
```

### 2. An audio control panel (knobs, faders, labels)

Each knob is a fixed-size frame with the pointer placed by `x`/`y` and `rotation`. Each fader is a track plus a thumb positioned inside a column cell. Labels are centred text.

```jsx
<Document>
  <Page name="Surface">
    <Frame name="Channel Strip" w=360 h=260 fill="#1c1c1e" radius=10 layout="row" gap=24 padding=20 align="start">
      <Frame name="Gain" w=64 h=90 layout="column" gap=8 align="center">
        <Frame name="Knob" w=64 h=64>
          <Ellipse name="Body" w=64 h=64 fill={[{"kind":{"RadialGradient":{"stops":[{"position":0,"color":"#4a4a4e"},{"position":1,"color":"#2a2a2d"}]}}}]} stroke="#000000" stroke-weight=2 />
          <Rectangle name="Pointer" x=30 y=6 w=4 h=22 radius=2 fill="#ff5a36" />
        </Frame>
        <Text name="Label" w=64 h=16 size=11 text-align="center" fill="#c7c7cc">GAIN</Text>
      </Frame>
      <Frame name="Pan" w=64 h=90 layout="column" gap=8 align="center">
        <Frame name="Knob" w=64 h=64>
          <Ellipse name="Body" w=64 h=64 fill="#2a2a2d" stroke="#000000" stroke-weight=2 />
          <Rectangle name="Pointer" x=48.97 y=12.2 w=4 h=22 radius=2 fill="#ffffff" rotation=45 />
        </Frame>
        <Text name="Label" w=64 h=16 size=11 text-align="center" fill="#c7c7cc">PAN</Text>
      </Frame>
      <Frame name="Volume" w=40 h=220 layout="column" gap=8 align="center">
        <Frame name="Fader" w=40 h=196>
          <Rectangle name="Track" x=17 y=0 w=6 h=196 radius=3 fill="#0e0e10" />
          <Rectangle name="Thumb" x=0 y=120 w=40 h=18 radius=3 fill="#d1d1d6" stroke="#8e8e93" stroke-align="inside" />
        </Frame>
        <Text name="Label" w=40 h=16 size=11 text-align="center" fill="#c7c7cc">VOL</Text>
      </Frame>
    </Frame>
  </Page>
</Document>
```

### 3. A fragment to insert: meter with a gradient and a mixed-style label

A gradient without a `transform` runs left to right across the node, so this meter is horizontal.

```jsx
<Frame name="Meter" w=160 h=16 fill="#0e0e10" radius=2 clip>
  <Rectangle name="Level" w=112 h=16 fill={[{"kind":{"LinearGradient":{"stops":[{"position":0,"color":"#34c759"},{"position":0.7,"color":"#ffcc00"},{"position":1,"color":"#ff3b30"}]}}}]} />
</Frame>
<Text name="Readout" w=80 h=16 size=12 fill="#ffffff" runs={[{"text":"-6.0"},{"text":" dB","fills":["#8e8e93"]}]}>-6.0 dB</Text>
```

## Light and shader effects (optional)

- `emission={{"color":"#ff5a2a","intensity":1.4,"radius":12}}` — the node gives
  off light (VU lamps, jewels, lit buttons, LEDs). GPU renderers bloom it onto
  what's around it; others draw a soft glow. It scales with the node's opacity,
  so a control's `Led` slot dimming a lamp dims its light.
- `shader={{"shader":"glass","params":{"strength":0.6}}}` — a post-effect on the
  node's subtree. Built-ins: `glass`, `crt`, `lighting`, `flicker`;
  `"asset:<id>"` runs custom WGSL from the document's assets (the contract:
  `features/render/render/src/fx/CONTRACT.md`). Optional: a surface must read
  correctly without it (non-GPU renderers skip shaders).


## Motion (animation)

`motion={[…]}` (or one `motion={{…}}`) animates a node. Each motion writes
one **target** from an **input** expression through **dynamics**, output
0..1 mapped onto `from`..`to`:

```jsx
<Frame name="Reel" w=60 h=60 motion={{"target":"rotate","input":"0.3 + 1.2*norm(tape/speed)","dynamics":"spin","to":360}} />
<Rectangle name="Needle" w=2 h=40 motion={{"target":"rotate","input":"norm(wah/position)","dynamics":{"spring":{"stiffness":260,"damping":14}},"from":-20,"to":20,"pivot":{"x":0.5,"y":1}}} />
<Ellipse name="Lamp" w=12 h=12 motion={{"target":"opacity","dynamics":{"lfo":{"rate":"param(chorus/rate)"}},"from":0.2}} />
<Frame name="Coil" w=400 h=60 motion={[{"name":"ring","target":"none","dynamics":{"oscillator":{"freq":5,"decay":"param(spring/decay)","excite":"level(spring/in)"}}},{"target":"y","input":"motion(ring)","to":8}]} />
```

- **target**: `none` (a named signal for `motion(name)`), `x`, `y` (px),
  `rotate` (°), `scale`, `stretch_x`, `stretch_y`, `skew_x`, `skew_y`,
  `opacity`, `emission`, `dash_offset`, `{"fill":{"colors":["#…","#…"]}}`,
  `{"stroke":{"colors":[…]}}`, `{"morph":{"shapes":["M…","M…"]}}`,
  `{"shape":{"provider":"coil|reverb.room|wave","part":"…","inputs":{…}}}`.
  Rotate/scale/stretch/skew turn about `pivot` (fractions of the node's box,
  default centre). Rotating containers must be `Frame`s.
- **input**: `norm(addr)`, `param(addr)`, `level(feed[,ch])`, `gr(feed)`,
  `scalar(feed)`, `band(feed,lo,hi)`, `feed(feed,i)`, `motion(name)`, `time`,
  `hover`, `press`, numbers, `+ - * / ( )`, `clamp min max abs lerp smoothstep
  step pow sqrt sin cos floor fract sign curve or`. A feed with no data is
  missing and makes the sum missing — wrap optional ones: `or(level(x/out), 0)`.
- **dynamics**: `"follow"` (default), `{"spring":{stiffness,damping,mass}}`,
  `{"oscillator":{freq,decay|damping,excite,gain}}` (rings when `excite`
  rises), `{"lfo":{rate,shape,phase}}`, `{"noise":{rate,amount}}`,
  `{"ballistic":{attack,release}}`, `{"keyframes":{times,values,easing,loop}}`,
  `"spin"` (input in turns/s). Any number may be an expression.
- `lag: true` adds only the dynamics' overshoot to what a control already
  shows (a spring on a needle). `name` lets others read it with `motion(name)`.

The `animate` tool edits motions; `lint` checks their sources.

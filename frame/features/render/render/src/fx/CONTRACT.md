# Effect shader contract

A node's `shader={{"shader":"<id>","params":{…}}}` runs a WGSL effect over the
node's subtree on the GPU effects stage (`render::fx`, `Mode::Gpu`). `<id>` is
a built-in (`glass`, `crt`, `lighting`, `flicker`, the visualizers,
`knob.3d`) or `asset:<asset id>`: the
UTF-8 WGSL stored in the document's asset table. Custom and built-in effects
use exactly the same contract.

## What the stage does

1. The node's subtree (its own paint and its descendants', with the enclosing
   groups, clips, layers and masks reapplied) renders alone into a
   transparent, frame-sized texture — the **source**.
2. A pass runs over the node's box (after its world transform —
   rotation/scale included — as a pixel rectangle; the rest of the frame
   gets the source passed through unchanged, so overflowing children still
   draw plainly). It calls your `effect` for every pixel of that rectangle,
   in uniform control flow (derivatives — `fwidth`, `dpdx` — are allowed),
   and keeps the result inside the box.
3. The result is composited *over* what was painted before the node, in paint
   order; later siblings paint on top of it.

The effect sees only its own subtree, not what's behind the node. Effects
must be optional: renderers without the stage (SVG export, `--fx fallback`,
`--fx off`) draw the subtree plainly.

## What you write

The body of one function (plus any helpers of your own):

```wgsl
fn effect(uv: vec2<f32>) -> vec4<f32>
```

- `uv` — node-local, `0..1` across the node's box (`(0,0)` = its top-left).
- Return a **premultiplied** RGBA colour. The stage clamps `rgb` to `0..a`
  and scales the result by the node's effective opacity (its own × its
  ancestors'), as the subtree it replaces would have been.
- Sample with `textureSampleLevel` (or the helpers below).
- Don't declare bindings or entry points; the stage prepends the prelude
  below and appends `fx_vs_main`/`fx_fs_main`. Names starting with `fx_` are
  reserved.

## Prelude (in scope for your code)

```wgsl
struct FxUniforms {
    time_s: f32,          // seconds since the host started (animates `flicker`)
    value: f32,           // position 0..1 of the nearest control (this node or
                          // an ancestor); its default, else 0
    px_per_unit: f32,     // target px per node unit (render scale × transform)
    param_mask: u32,      // bit i set = param slot i given by the document
    resolution: vec2<f32>,  // target size, px
    node_size: vec2<f32>,   // the node's box, node units
    to_local_x: vec4<f32>,  // target px → node-local units (rows of a 2×3;
                            //   `.w` = 1 − the node's effective opacity
    to_local_y: vec4<f32>,  //   affine: local = (dot(x.xyz, (px,1)), dot(y.xyz, (px,1))))
    to_px_x: vec4<f32>,     // node-local units → target px
    to_px_y: vec4<f32>,
    params: array<vec4<f32>, 4>,  // the 16 param slots, `params[i / 4][i % 4]`
    inputs: array<vec4<u32>, 8>,  // input slot i: (offset, len) into fx_data at
                                  //   inputs[i / 2].xy (even i) / .zw (odd i)
}

@group(0) @binding(0) var<uniform> fx: FxUniforms;
@group(0) @binding(1) var src_tex: texture_2d<f32>;  // the source, straight alpha
@group(0) @binding(2) var src_samp: sampler;          // linear, clamp-to-edge
@group(0) @binding(4) var<storage, read> fx_data: array<f32>;  // the inputs' values
```

Helpers:

| helper | returns |
|---|---|
| `sample_src(uv)` | source colour at node-local `uv`, premultiplied, bilinear |
| `sample_src_px(px)` | source colour at target pixel `px`, premultiplied |
| `uv_to_px(uv)`, `local_to_px(l)`, `px_to_local(px)` | coordinate mappings |
| `param(i)`, `param_set(i)`, `param_or(i, fallback)` | param slot `i` |
| `luminance(rgb)` | Rec. 709 luma |
| `hash11(x)` | cheap `0..1` hash |
| `node_px()` | the node's box in target pixels (`node_size · px_per_unit`) |
| `input_len(IN_X)` | how many values input `x` carries this frame (0 = no data) |
| `input_raw(IN_X, k)` | its value `k` (clamped to its length; 0 with no data) |
| `input_or(IN_X, fallback)` | its first value, or `fallback` with no data — a param, a reading |
| `input_at(IN_X, u)` | read across `u` 0..1 (first value at 0, last at 1), linear |

## Params

`params` in the document are named; each name has a fixed slot (and a
`P_<NAME>` constant). Names not in this table are ignored.

| slot | name | constant |
|---:|---|---|
| 0 | `strength` | `P_STRENGTH` |
| 1 | `speed` | `P_SPEED` |
| 2 | `amount` | `P_AMOUNT` |
| 3 | `scale` | `P_SCALE` |
| 4 | `angle` | `P_ANGLE` |
| 5 | `elevation` | `P_ELEVATION` |
| 6 | `softness` | `P_SOFTNESS` |
| 7 | `radius` | `P_RADIUS` |
| 8–11 | `tint_r`, `tint_g`, `tint_b`, `tint_a` | `P_TINT_R` … `P_TINT_A` |
| 12–15 | `p0`, `p1`, `p2`, `p3` | `P_P0` … `P_P3` |

Use `param_or(P_STRENGTH, 0.6)` to give a param a default.

## Inputs: live data

A shader can read arrays of live data — feed frames (spectrum bins, a
gain-reduction history, a scope, envelope voices), param values, a curve
provider's series — uploaded every frame. The document names them on the
node, in `ShaderFx::inputs` (additive; absent = none, the old shape):

```text
shader={{"shader":"viz.comp","params":{…},"inputs":[
  {"name":"threshold","source":{"Param":{"addr":"comp/threshold"}}},
  {"name":"gr","source":{"Stream":{"id":"comp/gr_history","channel":null}},
   "len":128,"map":{"lo":0,"hi":-60}},
  {"name":"left","source":{"Curve":{"provider":"delay.taps","inputs":[…],"series":"left"}}}]}}
```

| source | values |
|---|---|
| `Param { addr }` | the param in engineering units (a choice's index, a toggle's 0/1): one value |
| `Stream { id, channel }` | the feed's latest frame — all of it, or `channel`'s value |
| `Curve { provider, inputs, series }` | a curve provider's series (the runtime's registry, as a `Curve` control evaluates it), flattened `x0, y0, x1, y1, …` |
| `Const { values }` | fixed numbers |

`map: { lo, hi }` maps each value `lo..hi → 0..1` (clamped unless
`unclamped`; `hi < lo` runs the other way), then `len: n` resamples to
exactly `n` values (linear, both ends kept — a fixed-length uniform array's
shape); non-finite values arrive as 0. At most 1024 values per input, 16
inputs and 4096 floats (16 KiB) per node per frame.

**Naming.** Every `IN_<NAME>` your source mentions gets a slot — sorted,
declared by the stage as `const IN_<NAME>: u32` — and is fed by the input
whose `name` upper-cases to `<NAME>` (`gr_trace` → `IN_GR_TRACE`). An input
the document doesn't bind, or that has no data yet, reads as empty
(`input_len == 0`), so give every read a default (`input_or`).

**Who resolves them.** The host: `surface_runtime::Runtime::shader_inputs()`
returns every shader node's values (`render::fx::FxInputs`), which go in
`Frame::inputs`; `Runtime::set_clock(seconds)` is the clock animated curve
providers read. Without a host (a plain render of a document) every input is
empty — except `Const` inputs, which the stage takes from the document
itself (mapped and resampled the same way) when the host gave none: a
shader's fixed look (a preset table) needs no host. Renderers without the
stage ignore inputs entirely: the node's subtree — the vector fallback —
draws.

## Built-ins

| id | does | params (defaults) |
|---|---|---|
| `glass` | shallow lens refraction + diagonal specular sheen and top rim | `strength` (0.6) |
| `crt` | scanlines, slight barrel, phosphor bloom, vignette | `strength` (0.7), `scale` = scanline period in node units (3) |
| `lighting` | directional light over a height field from luminance (screen-space normals, so a turning cap keeps its light) | `strength` (0.8), `angle` (135° = from the top-left), `elevation` (45°), `scale` = bump depth (1) |
| `flicker` | tube-lamp brightness jitter from `time_s`; the node's (and its subtree's) `emission` flickers with it | `strength` = depth (0.5), `speed` Hz (10) |

### Visualizers

processor's panel shaders (`comp-ui`, `delay-ui`, `reverb-ui`,
`modulation-ui`, `eq-ui`), ported with their maths and colours unchanged
(`wgsl/viz/`; `common.wgsl` is processor's `hotter` / `tonemap`). They draw
their own picture from their inputs and **replace** the subtree — which is
the picture's vector fallback for renderers without the stage. The kit wraps
each as a component (`viz.comp_scope`, `viz.delay_taps`, `viz.reverb_space`,
`viz.mod_lfo`, `viz.eq_glow`; see `surface-kit/KIT.md`). All animate
(`time_s`); colour is `tint_r/g/b` (0..1).

| id | inputs | params |
|---|---|---|
| `viz.comp` | `threshold`, `ratio`, `knee` (dB), `in_db` (the ball), `gr_db` (≥ 0), `input`, `gr` (traces 0..1, 128), `on`, `grabbable` | `p0` = dB range (60) |
| `viz.delay` | `left`, `right` (taps, `at_s, level` pairs), `lane` (`window_s, beat_s`), `family` (0 Digital … 5 Special), `on` | — (rules its own beat grid) |
| `viz.reverb` | `decay` (s), `predelay` (ms), `density`, `damp`, `mix`, `bpm`, `family` (0 Room … 7 Convolution), `on` | — (rules its own beat grid) |
| `viz.modulation` | `rate` (Hz), `depth`, `mix`, `engine` (0 Chorus … 5 Rotary), `on` | — |
| `eq.glow` | `spectrum` (dB bins, 128), `bands` (`freq_hz, gain_db` pairs), `db_range`, `floor` | `strength` node glow, `amount` spectrum glow, `softness` hue spread, `radius` bloom (all 1); `p0`/`p1` the frequency axis (20, 20 000) |

### Rendered parts

| id | does | params | inputs |
|---|---|---|---|
| `knob.3d` | a knob sphere-traced per pixel from a distance field: cap, skirt, insert, pointer; PBR-ish shading under the kit's light; turns with `value`. **Replaces** the subtree (the vector `knob.render` knob, its fallback) | see below | `shape`, `skirt`, `insert`, `pointer`, `light` (`Const`), `value` |

**`knob.3d`.** The knob stands in the middle of the node's box: its
footprint (skirt, or the cap without one) is `radius` × half the box's
shorter side, and the rest of the box holds its lean and its shadow. The
view is oblique — a camera a little in front of the panel (`elevation`°
from straight down): the panel stays square, what stands off it leans up
and shows its front wall. Light: the kit's key from the upper left
(azimuth −35°, 50° up), a cool fill from the lower right, a rim/strip
light behind; a studio environment (softbox, strip, ceiling, lit panel)
for reflections; GGX specular (anisotropic for spun metal: highlights
stretch across the turning rings), a clearcoat lobe, one-tap occlusion in
grooves, a soft shadow on the skirt under the cap, and an analytic contact
+ cast shadow on the panel, faded before the box edge. The ribs, knurl,
beak and pointer turn by `(value − 0.5) × angle`; the light, highlights
and shadow don't. Edges are anti-aliased from the ray's closest approach
(`fwidth`), ribs finer than ~2.5 px average out. Output is premultiplied;
knob units have the footprint's radius 1.

| slot | meaning (default) |
|---|---|
| `strength` | specular strength (1) |
| `speed` | the cap's anisotropy, 0..1 — spun metal (0) |
| `amount` | the cap's metalness, 0..1 (0) |
| `scale` | the top edge's bevel radius (0.07) |
| `angle` | travel, degrees (300) |
| `elevation` | view tilt, degrees (30) |
| `softness` | the cap's roughness (0.38) |
| `radius` | footprint ÷ half the box (0.66) |
| `tint_r/g/b` | the cap's colour, sRGB 0..1 |
| `tint_a` | the cap's clearcoat, 0..1 (0) |
| `p0` | profile: 0 flat · 1 domed · 2 fluted · 3 knurled · 4 chicken head · 5 teardrop pointer cap · 6 wing (dome + fin) · 7 stepped · 8 fluted body + teardrop insert |
| `p1` | flutes or knurl teeth (0 = the profile's: 8, 60, 12) |
| `p2` | skirt: 0 none · 1 plain · 2 flared · 3 knurled · 4 metal · 5 dished |
| `p3` | pointer: 0 none · 1 line on the top · 2 dot · 3 line down the side too · 4 engraved groove (1) |

| input (`Const`) | values (defaults) |
|---|---|
| `shape` | cap radius (0.72), height (0.5), dome (0; 0.16 domed), rib depth (0.045; 0.02 knurled), knurl band 0..1 from the panel (1), step radius (0.5), step height (0.14), diamond knurl 0/1 (0), skirt teeth (48) |
| `skirt` | r, g, b, metal, roughness, clearcoat, radius (1), height (0.16) |
| `insert` | r, g, b, metal (1), roughness (0.2), anisotropy, size (0.62), height (0.09) |
| `pointer` | r, g, b, width (0.075), inner and outer end in cap radii (0.3, 0.92) |
| `light` | shadow (1), occlusion (1), fill (1), rim (1), key (1) |
| `value` | the position 0..1, overriding `value` (bind a param with a `map`) |

The kit's `knob.3d` component (`surface-kit`, `materials::knob3d`) fills
these from a preset table. Cost (RTX 4080, GPU timestamps): 30 knobs in
97-unit boxes ≈ 0.25 ms at 1×, 0.29 ms at 2× — each pass is a single
latency-bound wave, so the cost follows the number of knobs more than
their size.

## Generators

An effect that never reads its source — its body mentions neither
`sample_src*` nor `src_tex` (`knob.3d`, the visualizers) — draws its own
picture. The stage doesn't render its node's subtree at all on the GPU
path: no offscreen vello render and no blits, just the effect's pass
(`src_tex` is bound to a transparent texture). So a generator's subtree
doesn't overflow its box there either: it is only the fallback, for
renderers without the stage.

## Failure

A shader that doesn't parse, validate or build a pipeline (or an `asset:` id
with no such asset) is reported once on stderr (`frame fx: shader … drawing
its subtree plainly`) and its subtree draws as if it had no shader. Compiled
pipelines are cached by id (asset ids are content hashes).

## Time

`time_s` is the host's clock. A scene with a `flicker`, or a shader (built-in
or custom) whose source mentions `time_s`, is *time-dependent*
(`render::fx::time_dependent`): hosts redraw it continuously.

## Example: invert

```wgsl
fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = sample_src(uv);
    return vec4<f32>(vec3<f32>(c.a) - c.rgb, c.a);
}
```

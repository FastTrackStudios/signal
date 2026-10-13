# Ralph Progress Log

This file tracks progress across iterations. Agents update this file
after each iteration and it's included in prompts for context.

## Codebase Patterns (Study These First)

### SVG Color + Opacity Pipeline
- `svg_color_paint_values(r: u8, g: u8, b: u8, a: f64) -> (String, Option<String>)` — encodes RGB as hex, alpha via `fmt_f64(a)` (trims trailing zeros)
- `to_u8(v: f64) -> u8` — only used for RGB channels (hex rendering); never for alpha in SVG output
- `fmt_f64(v: f64) -> String` — 6 decimal places, trimmed (e.g., 0.7 not 0.700000)
- Rgba struct stores all channels as f64; alpha flows through without quantization in SVG path

### Parity Test Workflow
- `tools/render_parity_actuals.sh --all` — regenerates all actual.svg files
- `tools/run_svg_parity_scenarios.sh --all` — runs comparison and updates reports
- `actual.svg` files in scenario dirs are generated artifacts; can be stale after code changes
- Failures in: effects.effect-ordering, effects.layer-background-blur, primitives.fractional-geometry, regression.tough-1176-blue-face are pre-existing (not introduced by bd-der.1)

---

## 2026-02-27 - bd-der.1
- **What was implemented**: Eliminated 8-bit opacity quantization in SVG export
- **Files changed**:
  - `crates/frame-ui/src/svg/render.rs` — changed `svg_color_paint_values` signature from `a: u8` to `a: f64`, updated all call sites to pass `color.a` directly instead of `(color.a * 255.0).round() as u8`
  - `crates/frame-ui/src/svg/effects.rs` — same fix for stroke color alpha in push_rect_with_effects
  - `tests/frame/parity/scenarios/regression.tough-1176-blue-face/actual.svg` — regenerated with clean opacity values
  - `tests/frame/parity/scenarios/text.alignment-truncation/actual.svg` — regenerated, now passes strict parity
  - Report files updated
- **Learnings**:
  - The fix was already present in the working tree (previous iteration had implemented it); this iteration verified correctness and regenerated stale artifacts
  - The old code did `(color.a * 255.0).round().clamp(0.0, 255.0) as u8` as the 4th arg to svg_color_paint_values, which stored alpha as u8 and then formatted as `f64::from(a) / 255.0` with 6 decimal places — creating values like 0.698039 instead of 0.7
  - Figma JSON opacity values come in as f32-precision (e.g., 0.699999988079071 for 0.7); `fmt_f64` correctly rounds these to "0.7" via `format!("{:.6}", v)` + trim trailing zeros
  - actual.svg parity files are GENERATED artifacts — always regenerate after code changes before running comparison
  - text.alignment-truncation was also fixed as a side effect (now strict=true)

---

## 2026-02-27 - bd-der.3
- **What was implemented**: Placed opacity on `<g filter="...">` wrapper instead of baking it into fill/effect colors
- **Files changed**:
  - `crates/frame-ui/src/anyrender_renderer/collect.rs` — added `has_filter_effects` early check; `needs_layer` now also true when `has_filter_effects && effective_opacity < 1.0`, so opacity is NOT baked into fills/effects for those nodes
  - `crates/frame-ui/src/svg/render.rs` — modified `LayerEnd` to merge outer opacity group into inner filter group child; added `try_merge_layer_into_filter_child()` and `push_node_dyn()` helpers
- **Learnings**:
  - Figma's SVG convention: when a node has both opacity AND filter effects, opacity goes on the SAME `<g filter="...">` element, NOT baked into fill/stroke/effect colors
  - The `svg` crate's `Node` trait provides `get_attributes_mut()` and `get_children_mut()` for DOM inspection — `Box<dyn Node>` is usable via trait object dispatch
  - `Box<dyn Node>: Into<Box<dyn Node>>` via std identity impl, so `group.add(child_box)` compiles
  - Pre-existing `needs_layer` in collect.rs was only for non-Normal blend modes; adding filter-effects case required peeking at raw JSON before full effect parsing
  - The merge check: require outer group has opacity, exactly 1 child, child is `<g>` with `filter` but no `opacity`; copy all outer attrs to child using `entry().or_insert()`
  - SVG attribute order differs (actual: `filter opacity`, expected: `opacity filter`) but is semantically equivalent — canonical diff still succeeds
  - 22/26 parity tests still pass; same 4 pre-existing failures remain unchanged

---

## 2026-02-27 - bd-der.4
- **What was implemented**: Applied rotation transform to star and vector shape paths
- **Files changed**:
  - `crates/frame-ui/src/svg/render.rs` — inserted rotation block after px/py candidate correction (before bounds accumulation). When `rotation.abs() > 1e-6 && has_native_geometry && !text_outline`, applies 2×2 rotation to all fill_paths/stroke_paths via `kurbo::Affine` and recomputes `px/py` as `chain_sx/chain_sy`
- **Key formulas**:
  - `cos_r = rotation.cos()`, `sin_r = rotation.sin()` (rotation from extract_local_rect = atan2(c, a))
  - `abs_cx = bx + tx + bw/2`, `abs_cy = by + ty + bh/2` (absoluteBoundingBox center in screen space)
  - `chain_sx = abs_cx - cos_r*(w/2) + sin_r*(h/2)` — positions rotated local center at abs center
  - `chain_sy = abs_cy - sin_r*(w/2) - cos_r*(h/2)`
  - `kurbo::Affine::new([cos_r, sin_r, -sin_r, cos_r, 0.0, 0.0])` — column-major 2×2 rotation only
- **Learnings**:
  - Figma's relativeTransform tx/ty is the AABB top-left, NOT the local origin — the correct translate uses absoluteBoundingBox center
  - Decompose affine = 2×2 rotation on path coords + translate; normalize bakes the translate into compact path format
  - kurbo Affine::new([a,b,c,d,e,f]) is column-major: `|a c e| / |b d f|`; Figma row-major [[cos_r, -sin_r],[sin_r, cos_r]] → `new([cos_r, sin_r, -sin_r, cos_r, 0, 0])`
  - `match primitive` (not `match &primitive`) means fill_paths/stroke_paths are owned Vec<String>; shadow with `let (fill_paths, stroke_paths) = ...;` after candidate correction
  - 22/26 parity tests still pass; same 4 pre-existing failures; star M56.3732 37.7679 and Union paths now match expected.svg exactly

---

## 2026-02-27 - bd-der.5
- **What was implemented**: Emitted fill-opacity on pattern overlay elements; inlined blend mode on shape instead of wrapper `<g>`
- **Files changed**:
  - `crates/frame-ui/src/svg/render.rs` — 8 image fill code paths fixed:
    1. Removed `opacity` from `<image>` elements inside pattern defs (moved to consumer shape)
    2. Added `fill-opacity` to rect/circle when `*alpha < 1.0`
    3. Inlined `style="mix-blend-mode:..."` directly on shape (circle/rect) instead of wrapping in `<g>`
    4. Fixed paths: aggregate-group rect, single-fill rect, FRAME multi-fill for-loop, main RECTANGLE path, circle path, 3x path fills
- **Learnings:**
  - **4 distinct rendering paths** for image pattern fills in render.rs, each needed a separate fix: (1) `fills.len() > 1 && !effects.is_empty()` aggregate, (2) `fills.len() == 1` single-fill branch, (3) FRAME fills `for (fill, fill_blend) in fills` loop, (4) `is_rectangle_node` main rect path
  - **Image caching bug**: The original code put `opacity` on the `<image>` element, but `image_defs` caches by hash — subsequent shapes sharing the same image couldn't override its opacity. Moving to `fill-opacity` on the shape fixes this caching issue
  - **VU meter FRAME has 2 fills**: GRADIENT_LINEAR (normal blend) + IMAGE (multiply blend). `fills.len() > 1 && !effects.is_empty()` → false (no effects), so falls through to the FRAME for-loop path (not the single-fill path or RECTANGLE path)
  - **PASS_THROUGH → BlendMix::Normal**: `parse_blend_mode` maps `PASS_THROUGH` to `Normal` (no case for it), so PASS_THROUGH nodes don't trigger `needs_layer`. The blend mode wrapper for VU meter was coming from the fill-level blend in the FRAME for-loop, not a `LayerStart`
  - 22/26 parity tests still pass; same 4 pre-existing failures unchanged

---

## 2026-02-27 - bd-der.6
- **What was implemented**: Fixed angular gradient foreignObject sizing, conic-gradient angle, SVG transform matrix, seamless stop wrap, and metadata circle emission
- **Files changed**:
  - `crates/frame-ui/src/svg/render.rs` — 5 fixes in GradientAngular CIRCLE arm + `svg_angular_metadata_attr` function overhaul
  - `crates/frame-ui/src/svg/normalize.rs` — `normalize_circle_line` now preserves `data-figma-gradient-fill` attribute
- **Key changes in render.rs**:
  1. from-angle: was 90°-θ_grad (gave -45deg), now correctly `90deg` (conic-gradient starts at 12 o'clock, which maps to Figma's gradient angle 0 → SVG 90deg)
  2. FO size: was 2022.22 (based on `1/(s*s)` formula), now `(r + strokeWeight) / |a|` where a = t[0][0]*rw/2000 — correct scaling
  3. SVG transform matrix: was missing full rotation, now `matrix(a b c d cx cy)` with a=t[0][0]*rw/2000, b=-t[1][0]*rw/2000, c=-t[0][1]*rw/2000, d=t[1][1]*rw/2000
  4. Seamless stop wrap: added first stop at 0deg and last stop at 360deg to prevent gap
  5. `svg_angular_metadata_attr` reworked: stopsVar→[], full f64 precision via `{:?}` format, actual gradientTransform matrix passed for CIRCLE (None for rects)
- **Metadata transform formula** (column-major naming mRC where R=col, C=row):
  - m00 = t[0][0]*rw,  m01 = t[1][0]*rw,  m02 = cx - t[0][0]*rw
  - m10 = t[0][1]*rh,  m11 = t[1][1]*rh,  m12 = cy
- **normalize.rs root cause**: `normalize_circle_line` only re-emits known attrs — `data-figma-gradient-fill` was silently stripped. Fixed by adding `extract_xml_attr` call + conditional emit (same pattern as `normalize_rect_line`)
- **Learnings**:
  - `normalize_svg_for_figma_parity` is a post-processor that parses SVG line-by-line and only keeps known attributes per element type — any new attribute must be explicitly added to the normalizer function for its element type
  - SVG `{:?}` format for f64 preserves decimal point for whole numbers (1.0 not 1) and uses Ryu shortest-unique for fractionals; matches f32→f64 precision from Figma JSON better than `fmt_f64` (which rounds to 6dp)
  - The metadata circle IS generated correctly (confirmed via group-children trace) — the issue was the normalizer stripping it, not the renderer failing to emit it
  - 22/26 parity tests still pass; same 4 pre-existing failures; regression.tough-1176-blue-face remains a pre-existing failure (child count 201 vs 205 and path coord precision are unrelated to angular gradient work)

---

## 2026-02-27 - bd-der.7
- **What was implemented**: Fixed gradient type swaps (paint36-39) and missing paint42/43 defs
- **Files changed**:
  - `crates/frame-ui/src/svg/render.rs` — hoisted `raw_frame_strokes_early`/`has_complex_frame_strokes_early`/`frame_stroke_width_early` computation before the `fills.len()==1` shortcut block; in `GradientLinear` and `GradientRadial` single-fill arms, added check `frame_rect_uses_transform && has_complex_frame_strokes_early && frame_stroke_width_early > 0.0` to emit fill-only rect + call `push_complex_frame_stroke_overlays`; removed duplicate computation at original site (replaced with aliases)
- **Learnings:**
  - **Early-exit bug pattern**: The `fills.len()==1` shortcut hits `continue` before reaching the `raw_frame_strokes` computation at the original site. Any FRAME node with 1 gradient fill + gradient stroke skipped complex stroke overlay logic entirely — causing all subsequent paint IDs to shift by the count of missing stroke gradients
  - **FRAME stroke convention**: Figma SVG emits separate rects for each stroke (solid + gradient) as stroke-only rects (`fill="none"`), not combined with the fill rect. `push_complex_frame_stroke_overlays` handles both solid and gradient strokes in one loop
  - **Gradient ID numbering ripple**: The "swapped types" (paint36 radial vs linear) were actually a numbering offset caused by 2 missing linear gradient defs (MN frame's gradient stroke + Frame 2's gradient stroke). Once those were emitted, downstream IDs 36-43 shifted into correct positions with correct types
  - `push_complex_frame_stroke_overlays` only needed for FRAME nodes (`frame_rect_uses_transform = true`); non-FRAME nodes (circles, etc.) use the old combined fill+stroke rect approach

---

## 2026-02-27 - bd-der.8
- **What was implemented**: FRAME stroke rect deferred outside clip group (Figma places stroke after clip boundary, not inside it)
- **Files changed**:
  - `crates/frame-ui/src/svg/render.rs` — added `clip_depth: usize` counter + `pending_post_clip_stroke: Vec<(usize, Rectangle)>` queue; `ClipStart` increments depth, `ClipEnd` decrements depth and drains matching pending strokes after closing the group; in multi-fill loop stroke emission (line ~1446), when `frame_rect_uses_transform && clip_depth > 0`, push to pending queue instead of calling `push_node` directly
- **Learnings:**
  - **ClipStart/ClipEnd wraps Rect**: primitive order for FRAME with `clip_children=true` is `ClipStart → Rect (fills+stroke) → [children] → ClipEnd`. The stroke rect was landing inside the clip group.
  - **Figma convention**: FRAME stroke is a sibling of the clip group (after it), not a child inside it. Fills are correctly inside the clip (they get clipped to the boundary). The stroke is outside so it renders over the full boundary.
  - **Depth-tagged deferred queue**: `Vec<(usize, Rectangle)>` where the usize is `clip_depth` at deferral time. `ClipEnd` only drains items whose depth == `old_depth` (the depth before decrement), handling nested clip groups correctly.
  - **Affects only multi-fill loop path**: single-fill shortcut `fills.len() == 1` with complex frame strokes uses `push_complex_frame_stroke_overlays` which emits before the FRAME's own `ClipStart` (so already outside the clip group). The multi-fill path emits after `ClipStart` → was landing inside.
  - **Child count result**: `/svg/g[0]` child count mismatch `actual=204 expected=205` is now resolved. Remaining 1176 failure is a path coordinate precision issue (pre-existing).
  - 22/26 parity tests still pass; same 4 pre-existing failures unchanged

---

## 2026-02-27 - bd-der.9
- **What was implemented**: Inlined mix-blend-mode on elements instead of wrapping in `<g>`
- **Files changed**:
  - `crates/frame-ui/src/svg/render.rs` — 4 fixes:
    1. `push_node_with_optional_blend`: replaced `Group::new().set("style",...).add(node)` with `svg::Node::assign(&mut node, "style", ...)` to set style directly on the node
    2. Text-outline path fill loop: changed group-wrap to `path = path.set("style",...)` then `path_group.add(path)`
    3. Nonfull-ellipse path fill loop: collapsed blend/no-blend branches into unified loop that sets `style` directly on each path
    4. Main path fill loop (non-special case): same inline fix as #2
- **Learnings:**
  - **Most fixes already done**: bd-der.5 had already inlined blend mode for image pattern fills (circle/rect); `try_merge_layer_into_filter_child` already handled filter+opacity+blend merging. Only the path fill loops and `push_node_with_optional_blend` remained.
  - **`svg::Node::assign(&mut node, key, val)`** works for any `N: svg::Node` including groups — used to mutate the node before passing to `push_node`, eliminating the wrapper `<g>` entirely
  - **Angular group callers** are also correct: `push_node_with_optional_blend` now sets `style` on the existing group object (no extra nesting)
  - **Semantic equivalence**: For multi-path fills, setting blend mode on each individual path is correct since Figma inlines per-path
  - 22/26 parity tests still pass; same 4 pre-existing failures unchanged
---

## 2026-02-27 - bd-der.2
- **What was implemented**: Fixed matrix transform circle translations for reflected/flipped circles
- **Files changed**:
  - `crates/frame-ui/src/svg/render.rs` — in the `det < -1e-6` circle branch, replaced `e = row0[2] + canvas_tx` / `f = row1[2] + canvas_ty` with `e = cx - (a + c) * r` / `f = cy - (b + d) * r`, back-calculating world-space translation from the absoluteBoundingBox center
- **Learnings**:
  - **Root cause**: For reflected circles (det < -1e-6), render.rs read `relativeTransform.tx/ty` (local to parent) and added canvas offset — ignoring the parent rotation chain. For a circle inside a rotated parent, the local tx/ty in parent space ≠ world tx/ty
  - **Fix without collect.rs changes**: `absoluteBoundingBox` center (already available as `cx, cy` from `source_bboxes`) IS the world-space center. Back-calculate: for SVG `matrix(a,b,c,d,e,f)` mapping local (r,r) → world (cx,cy): `e = cx-(a+c)*r`, `f = cy-(b+d)*r`
  - **`det < -1e-6` = reflection case**: Pure rotations have det=+1 (handled by `is_rotation` path using `rotate(angle cx cy)`); reflections have det=-1 (requires full `matrix(...)`)
  - **absoluteBoundingBox center is always correct for round circles**: Rotation/reflection doesn't change a circle's AABB, so the AABB center is always the world center regardless of the transform chain
  - **Variable naming in circle block is SVG column-major**: `a`=row0[0]=SVG_a, `c`=row0[1]=SVG_c, `b`=row1[0]=SVG_b, `d`=row1[1]=SVG_d — emitted as `matrix(a b c d e f)` = SVG format directly
  - All 16 reflected circles in 1176 now match expected translations within 0.001 (abs_tol 0.006); 22/26 parity tests still pass; same 4 pre-existing failures

---

## 2026-02-27 - bd-der.10
- **What was implemented**: Introduced `AffineTransform2D` type replacing all raw `[[f64; 3]; 2]` arrays in the render pipeline
- **Files changed**:
  - `crates/frame-ui/src/anyrender_renderer/transform.rs` (new) — `AffineTransform2D` struct with fields `a, b, tx, c, d, ty`; `impl Mul` for composition; `impl Display` for SVG `matrix()` string; `impl From<[[f64;3];2]>`; methods: `identity()`, `rotation()`, `translation()`, `scale()`, `transform_point()`, `determinant()`, `is_identity()`, `decompose_rotation()`, `inverse()`, `to_svg_matrix_coeffs()`; 17 unit tests
  - `crates/frame-ui/src/anyrender_renderer/mod.rs` — added `pub mod transform` and re-export of `AffineTransform2D`
  - `crates/frame-ui/src/anyrender_renderer/types.rs` — `FillPaint` gradient/image variants now use `Option<AffineTransform2D>`
  - `crates/frame-ui/src/anyrender_renderer/extract.rs` — `gradient_transform_from_paint` and `parse_image_transform` return `Option<AffineTransform2D>`; `ImageFillData.image_transform` field updated
  - `crates/frame-ui/src/anyrender_renderer/paint.rs` — all gradient/image function signatures updated; field access replaces array indexing
  - `crates/frame-ui/src/svg/render.rs` — all gradient/image function signatures updated; field access replaces array indexing; also found a second raw-array usage in angular gradient CIRCLE branch
  - `crates/frame-ui/src/svg/source_bridge.rs` — `raw_gradient_transform_from_paint` returns `AffineTransform2D`
- **Learnings:**
  - **Figma row-major vs SVG column-major**: Figma `t[row][col]` → our fields `a=t[0][0], b=t[0][1], tx=t[0][2], c=t[1][0], d=t[1][1], ty=t[1][2]`. SVG `matrix(a,b,c,d,e,f)` is column-major — output order is `(a, c, b, d, tx, ty)` via `to_svg_matrix_coeffs()`
  - **Axis vectors for radial/angular gradients**: column vectors of the 2×2 linear part scaled to pixel space: `ax = (t.a * width, t.c * height)`, `ay = (t.b * width, t.d * height)` — NOT `t[0][*]` / `t[1][*]` per row
  - **`From<[[f64;3];2]>` as bridge**: lets parsing code construct `AffineTransform2D` from Figma JSON arrays without touching the rest of the call chain
  - **Near-zero determinant test**: `1e-7 * 1e-7 = 1e-14 < 1e-12` threshold — IS singular. The test comment "above threshold" was wrong; fixed test to match actual behavior
  - 30/30 unit tests pass; all pre-existing parity results unchanged
---

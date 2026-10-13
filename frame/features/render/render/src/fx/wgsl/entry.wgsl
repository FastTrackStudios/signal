
// ── entry points (appended by the effects stage) ─────────────────────────

struct FxVsOut {
    @builtin(position) pos: vec4<f32>,
}

@vertex
fn fx_vs_main(@builtin(vertex_index) i: u32) -> FxVsOut {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: FxVsOut;
    o.pos = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    return o;
}

@fragment
fn fx_fs_main(in: FxVsOut) -> @location(0) vec4<f32> {
    let px = in.pos.xy;
    let uv = px_to_local(px) / max(fx.node_size, vec2<f32>(1e-6));
    // Evaluated for every pixel (uniform control flow, so an effect may
    // use derivatives), then discarded outside the box.
    let c = effect(clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)));
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        // Outside the node's box the subtree draws plainly.
        return sample_src_px(px);
    }
    // Keep the result a valid premultiplied colour, at the node's opacity
    // (`to_local_x.w` = 1 − opacity).
    let a = clamp(c.a, 0.0, 1.0);
    let o = clamp(1.0 - fx.to_local_x.w, 0.0, 1.0);
    return vec4<f32>(clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(a)), a) * o;
}

// pane — the flat glass over a Time face's picture well. No lens: what is
// under it stays exactly where it is drawn (a handle in the picture stays
// under the finger). A window light from the upper left lays a soft sheen
// across the pane's left end, the top edge catches a bright rim, and the
// well darkens a little toward its edges, so the picture sits behind the
// glass. Reads no clock: a still picture renders once.
//
// params: strength (0.5) — the sheen and the rim; amount (0.5) — the
// edges' shade.

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let strength = clamp(param_or(P_STRENGTH, 0.5), 0.0, 1.0);
    let shade = clamp(param_or(P_AMOUNT, 0.5), 0.0, 1.0);
    let c = sample_src(uv);
    let size = node_px();
    let px = uv * size;
    // The sheen: a diagonal band, its slope in the pane's own proportions
    // (a wide pane catches the light at its left end only).
    let d = (px.y + px.x * 0.32) / max(size.y, 1.0);
    let sheen = smoothstep(0.78, 0.28, d) * smoothstep(0.02, 0.24, d) * 0.22;
    // The rim along the top edge, two pixels or so.
    let rim = (1.0 - smoothstep(0.0, 2.5, px.y)) * 0.28;
    // The edges' shade: within ~10 px of any edge.
    let edge = min(min(px.x, size.x - px.x), min(px.y, size.y - px.y));
    let dark = (1.0 - smoothstep(0.0, 10.0, edge)) * 0.35 * shade;
    let h = clamp((sheen + rim) * strength, 0.0, 1.0) * c.a;
    let rgb = c.rgb * (1.0 - dark) * (1.0 - h) + vec3<f32>(h);
    return vec4<f32>(rgb, c.a);
}

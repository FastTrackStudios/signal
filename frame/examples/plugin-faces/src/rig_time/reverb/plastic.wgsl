// plastic — the satin sheen of moulded ivory plastic on a slanted remote's
// control surface. The window light falls from above and to the left: a
// broad soft highlight pools on the upper left of the moulding, a faint
// satin band runs down across it, the top of the slant catches a little
// more light than the rest and the surface turns away from it toward the
// front lip, which falls into a soft shade. Matte, not glossy: nothing
// here is a mirror. Reads no clock: a still face renders once.
//
// params: strength (0.5) — the highlight and the satin band; amount (0.5)
// — the shade toward the front.

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let strength = clamp(param_or(P_STRENGTH, 0.5), 0.0, 1.0);
    let shade = clamp(param_or(P_AMOUNT, 0.5), 0.0, 1.0);
    let c = sample_src(uv);
    let size = max(node_px(), vec2<f32>(1.0, 1.0));
    let px = uv * size;
    // The pool of light: an ellipse far wider than tall, up and left.
    let q = vec2<f32>((px.x - size.x * 0.24) / (size.x * 0.5), (px.y - size.y * 0.02) / (size.y * 0.95));
    let pool = exp(-dot(q, q) * 2.4) * 0.2;
    // The satin band: soft, diagonal, low.
    let d = (px.y + px.x * 0.22) / size.y;
    let band = smoothstep(0.05, 0.4, d) * smoothstep(0.95, 0.45, d) * 0.06;
    // The slant: brighter at the top, turning into shade toward the lip.
    let top = (1.0 - smoothstep(0.0, 0.45, uv.y)) * 0.05;
    let lip = smoothstep(0.55, 1.0, uv.y) * 0.16 * shade;
    let lift = clamp((pool + band + top) * strength, 0.0, 1.0) * c.a;
    let warm = vec3<f32>(1.0, 0.985, 0.95);
    let rgb = mix(c.rgb * (1.0 - lip), warm, lift);
    return vec4<f32>(rgb, c.a);
}

// comp knob light — the kit's key light on a vector knob: a bevel round
// every edge (from the knob's coverage), a domed or flat top, a specular
// hotspot and a fill glint, and — on a spun-metal cap — the radial streak
// spun aluminium shows under a point light. Reads its subtree (the vector
// knob, which renderers without the stage draw as it is) and relights it;
// the light stays put while the knob turns. Static: no clock.
//
// The knob stands in the middle of the node's box. Units: the body's
// radius (`radius` × the box's shorter side) is 1.
//
// params: strength (0.8) the light; radius body radius ÷ box (0.25);
//         p0 dome 0..1 (0.35); p1 spun cap radius, body radii (0 = none);
//         p2 reach: only this far from the centre is lit (1.3);
//         softness roughness 0..1 (0.35); amount bevel width (0.07);
//         scale bump depth (1).

// Toward the key (upper left, 50° up) and the fill (lower right); y down.
const K_KEY: vec3<f32> = vec3<f32>(-0.3687, -0.5267, 0.7660);
const K_FILL: vec3<f32> = vec3<f32>(0.5514, 0.4511, 0.7017);

fn knob_height(px: vec2<f32>) -> f32 {
    let c = sample_src_px(px);
    let rgb = c.rgb / max(c.a, 1e-4);
    return c.a * (0.6 + 0.4 * luminance(rgb));
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let c = sample_src(uv);
    if (c.a <= 0.002) {
        return c;
    }
    let box = fx.node_size;
    let rb = clamp(param_or(P_RADIUS, 0.25), 0.02, 0.5) * min(box.x, box.y);
    let d = (uv - vec2<f32>(0.5)) * box / rb;
    let rr = length(d);
    let reach = param_or(P_P2, 1.3);
    let lit = 1.0 - smoothstep(reach - 0.12, reach, rr);
    if (lit <= 0.0) {
        return c;
    }
    let strength = clamp(param_or(P_STRENGTH, 0.8), 0.0, 2.0);
    let rough = clamp(param_or(P_SOFTNESS, 0.35), 0.03, 1.0);
    let dome = param_or(P_P0, 0.35);
    let spun = param_or(P_P1, 0.0);
    let bump = 2.2 * param_or(P_SCALE, 1.0);

    // The bevel: the height field's slope a bevel's width either side.
    let px = uv_to_px(uv);
    let e = max(param_or(P_AMOUNT, 0.07) * rb * fx.px_per_unit, 0.75);
    let gx = knob_height(px + vec2<f32>(e, 0.0)) - knob_height(px - vec2<f32>(e, 0.0));
    let gy = knob_height(px + vec2<f32>(0.0, e)) - knob_height(px - vec2<f32>(0.0, e));
    // The top: a dome rounds off to the body's rim.
    let inside = 1.0 - smoothstep(0.92, 1.02, rr);
    let tilt = d * dome * inside;
    let n = normalize(vec3<f32>(-gx * bump + tilt.x, -gy * bump + tilt.y, 1.0));

    // Diffuse, relative to a flat face (which the vector art already lit).
    let flat_d = K_KEY.z;
    let diff = max(dot(n, K_KEY), 0.0) / flat_d;
    let shade = mix(1.0, 0.45 + 0.55 * diff, 0.75 * strength);
    // Specular: the key's hotspot (Blinn), sharper the smoother; the fill's
    // weaker glint on the far side.
    let v = vec3<f32>(0.0, 0.0, 1.0);
    let shin = mix(140.0, 10.0, rough);
    let hk = normalize(K_KEY + v);
    let hf = normalize(K_FILL + v);
    let flat_s = pow(hk.z, shin);
    let spec_k = max(pow(max(dot(n, hk), 0.0), shin) - flat_s * 0.6, 0.0);
    let spec_f = pow(max(dot(n, hf), 0.0), shin * 0.7) * 0.3;
    var spec = (spec_k + spec_f) * mix(1.0, 0.35, rough);

    // Spun metal: a radial streak through the centre, toward the light and
    // away from it, where the turning rings face the light square on.
    if (spun > 0.0 && rr < spun) {
        let t = vec2<f32>(-d.y, d.x) / max(rr, 1e-3);
        let k = normalize(K_KEY.xy);
        let across = dot(t, k);
        let streak = exp(-across * across / 0.012) * (0.25 + 0.75 * smoothstep(0.05, 0.5, rr / spun));
        let edge = 1.0 - smoothstep(spun * 0.92, spun, rr);
        spec += streak * edge * 0.55;
    }

    let rgb = c.rgb * shade + vec3<f32>(1.0, 0.98, 0.95) * spec * c.a * strength;
    return vec4<f32>(mix(c.rgb, rgb, lit), c.a);
}

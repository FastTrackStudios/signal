// glass — a shallow convex lens over the subtree plus a specular sheen.
// params: strength (0.6) — refraction + highlight amount.

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let strength = param_or(P_STRENGTH, 0.6);
    let d = uv - vec2<f32>(0.5);
    // Refraction: pull samples toward the centre (magnify), less at the rim.
    let refr = d * (-0.07 * strength) * (1.0 - dot(d, d) * 1.6);
    let c = sample_src(uv + refr);
    // Specular: a soft diagonal band from the top-left plus a thin top rim.
    let diag = uv.x * 0.45 + uv.y;
    let sheen = smoothstep(0.62, 0.2, diag) * smoothstep(-0.02, 0.18, diag) * 0.32;
    let rim = (1.0 - smoothstep(0.0, 0.07, uv.y)) * 0.22;
    let h = clamp((sheen + rim) * strength, 0.0, 1.0) * c.a;
    return vec4<f32>(c.rgb * (1.0 - h) + vec3<f32>(h), c.a);
}

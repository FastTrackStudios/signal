// crt — a phosphor readout: scanlines, slight barrel distortion, phosphor
// bloom and a soft vignette.
// params: strength (0.7), scale (3) — scanline period in node px.

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let strength = clamp(param_or(P_STRENGTH, 0.7), 0.0, 1.0);
    let period = max(param_or(P_SCALE, 3.0), 1.0);
    let d = uv - vec2<f32>(0.5);
    let r2 = dot(d, d);
    let uvb = vec2<f32>(0.5) + d * (1.0 + 0.14 * strength * r2);
    let base_a = sample_src(uv).a;
    if (any(uvb < vec2<f32>(0.0)) || any(uvb > vec2<f32>(1.0))) {
        return vec4<f32>(0.0, 0.0, 0.0, base_a);
    }
    let c = sample_src(uvb);
    // Phosphor bloom: the bright parts bleed into a small ring.
    let o = 1.8 / max(fx.node_size, vec2<f32>(1.0));
    var g = vec3<f32>(0.0);
    g += sample_src(uvb + vec2<f32>(o.x, 0.0)).rgb;
    g += sample_src(uvb - vec2<f32>(o.x, 0.0)).rgb;
    g += sample_src(uvb + vec2<f32>(0.0, o.y)).rgb;
    g += sample_src(uvb - vec2<f32>(0.0, o.y)).rgb;
    g += sample_src(uvb + o * 2.0).rgb;
    g += sample_src(uvb - o * 2.0).rgb;
    g += sample_src(uvb + vec2<f32>(o.x, -o.y) * 2.0).rgb;
    g += sample_src(uvb + vec2<f32>(-o.x, o.y) * 2.0).rgb;
    let bloom = max(g / 8.0 - vec3<f32>(0.3), vec3<f32>(0.0)) * 0.9 * strength;
    let y = uvb.y * fx.node_size.y;
    let scan = 0.5 + 0.5 * cos(6.2831853 * y / period);
    let lines = mix(1.0, 0.5 + 0.5 * scan, strength);
    let vignette = 1.0 - r2 * 0.9 * strength;
    let rgb = (c.rgb * lines + bloom) * vignette;
    return vec4<f32>(rgb, max(c.a, base_a));
}

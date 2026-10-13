// lighting — a directional light over a height field taken from the
// subtree's luminance, so knob caps and brushed metal catch the light. The
// normals are taken in screen space: a rotating cap keeps its light fixed.
// params: strength (0.8), angle (135° — from the top-left), elevation (45°),
//         scale (1) — bump depth.

fn height_at(px: vec2<f32>) -> f32 {
    let c = sample_src_px(px);
    return luminance(c.rgb);
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let strength = clamp(param_or(P_STRENGTH, 0.8), 0.0, 2.0);
    let angle = radians(param_or(P_ANGLE, 135.0));
    let elevation = radians(clamp(param_or(P_ELEVATION, 45.0), 5.0, 90.0));
    let bump = 4.0 * param_or(P_SCALE, 1.0);
    let px = uv_to_px(uv);
    let e = max(fx.px_per_unit, 1.0);
    let hx = height_at(px + vec2<f32>(e, 0.0)) - height_at(px - vec2<f32>(e, 0.0));
    let hy = height_at(px + vec2<f32>(0.0, e)) - height_at(px - vec2<f32>(0.0, e));
    let n = normalize(vec3<f32>(-hx * bump, -hy * bump, 1.0));
    let l = normalize(vec3<f32>(cos(angle) * cos(elevation), -sin(angle) * cos(elevation), sin(elevation)));
    let flat_diffuse = sin(elevation);
    let diffuse = max(dot(n, l), 0.0);
    let h = normalize(l + vec3<f32>(0.0, 0.0, 1.0));
    let flat_spec = pow(h.z, 24.0);
    let spec = max(pow(max(dot(n, h), 0.0), 24.0) - flat_spec, 0.0);
    let c = sample_src_px(px);
    let shade = mix(1.0, diffuse / max(flat_diffuse, 0.2), strength);
    let rgb = c.rgb * shade + vec3<f32>(spec * 0.6 * strength) * c.a;
    return vec4<f32>(rgb, c.a);
}

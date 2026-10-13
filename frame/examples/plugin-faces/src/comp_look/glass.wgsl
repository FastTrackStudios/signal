// comp meter glass — the window over a meter: a shallow curved lens, the
// room's softbox reflected across its upper part, a bright rim along the
// top, a small glint in the corner and, for a backlit movement, the lamp
// glowing warm through the card from below. Reads its subtree (the card,
// scale and needle — what renderers without the stage draw) and covers it.
// Static: no clock (a moving needle re-renders it, nothing else does).
//
// params: strength (0.6) the reflection; p0 backlight 0..1 (0);
//         tint_r/g/b the lamp's colour (a warm 1, 0.9, 0.74);
//         p1 refraction (1).

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let s = clamp(param_or(P_STRENGTH, 0.6), 0.0, 2.0);
    let d = uv - vec2<f32>(0.5);
    let refr = d * (-0.045 * param_or(P_P1, 1.0)) * (1.0 - dot(d, d) * 1.6);
    var c = sample_src(uv + refr);

    // The lamp: warm light from below the card, brightest low centre,
    // falling off to the top corners.
    let bl = clamp(param_or(P_P0, 0.0), 0.0, 1.0);
    let lamp = vec3<f32>(param_or(P_TINT_R, 1.0), param_or(P_TINT_G, 0.9), param_or(P_TINT_B, 0.74));
    let g = exp(-pow(d.x / 0.62, 2.0) - pow((uv.y - 1.05) / 0.85, 2.0));
    let warm = mix(vec3<f32>(1.0), lamp * (0.86 + 0.34 * g), bl);
    // The window's depth: the card falls off into its corners.
    let vig = 1.0 - 0.16 * smoothstep(0.3, 0.78, length(d * vec2<f32>(1.0, 1.25)));
    c = vec4<f32>(c.rgb * warm * vig, c.a);

    // The softbox: a bright window over the upper left, its lower edge a
    // soft diagonal, fading toward the right.
    let size = fx.node_size;
    let aspect = size.x / max(size.y, 1.0);
    let edge = uv.y - (0.42 - 0.3 * uv.x) + 0.04 * sin(uv.x * 3.0);
    let box = (1.0 - smoothstep(-0.012, 0.012, edge)) * smoothstep(0.0, 0.12, uv.y);
    let reflect_k = box * (0.46 - 0.32 * uv.x);
    // A glint: the lamp of the room in the corner of the glass.
    let gp = (uv - vec2<f32>(0.14, 0.16)) * vec2<f32>(aspect, 1.0);
    let glint = exp(-dot(gp * vec2<f32>(0.7, 1.4), gp * vec2<f32>(0.7, 1.4)) / 0.003) * 0.32;
    // The rim: the glass's top edge, and a faint one down the sides.
    let rim = (1.0 - smoothstep(0.0, 0.05, uv.y)) * 0.28 + (1.0 - smoothstep(0.0, 0.02, min(uv.x, 1.0 - uv.x))) * 0.08;
    let h = clamp((reflect_k + glint + rim) * s, 0.0, 0.9) * c.a;
    // The glass itself takes a little light out of what is behind it, so
    // the reflection reads against it.
    let behind = c.rgb * (1.0 - 0.07 * s);
    return vec4<f32>(behind * (1.0 - h) + vec3<f32>(h), c.a);
}

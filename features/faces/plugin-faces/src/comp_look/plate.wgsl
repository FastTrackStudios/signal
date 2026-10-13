// comp plate light — the room's light on a compressor's faceplate, drawn
// over the plate's paint and under its parts. A generator: it never reads
// its subtree (an empty frame — renderers without the stage draw nothing),
// it returns premultiplied light and shade to lay over the plate.
//
// Brushed metal (`p0` = 1) shows the key light as a long anisotropic band
// across the brushing — the brushing is horizontal, so the band stands
// upright — streaked by the grain; painted metal (`p0` = 0) a broad satin
// lobe, and a softbox reflection whose sharpness is `softness` (gloss).
// Both get a lit top and left edge and a darker foot. Static: no clock.
//
// params: strength (0.5) the light; p0 brushed 0..1 (1); softness 0..1,
//         the paint's roughness (0.6); p1 band centre, uv x (0.3);
//         p2 grain contrast (1); p3 shade at the foot (0.18);
//         radius the plate's corner radius, node units (0);
//         tint_r/g/b the light's colour (a cool white).

fn plate_hash(p: vec2<f32>) -> f32 {
    let q = fract(p * vec2<f32>(0.1031, 0.1030));
    let r = q + dot(q, q.yx + 33.33);
    return fract((r.x + r.y) * r.x);
}

// Value noise, smooth across cells.
fn plate_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = plate_hash(i);
    let b = plate_hash(i + vec2<f32>(1.0, 0.0));
    let c = plate_hash(i + vec2<f32>(0.0, 1.0));
    let d = plate_hash(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let size = fx.node_size;
    let p = uv * size;
    let strength = clamp(param_or(P_STRENGTH, 0.5), 0.0, 2.0);
    let brushed = clamp(param_or(P_P0, 1.0), 0.0, 1.0);
    let rough = clamp(param_or(P_SOFTNESS, 0.6), 0.02, 1.0);
    let cx = param_or(P_P1, 0.3);
    let grain_k = param_or(P_P2, 1.0);
    let foot = param_or(P_P3, 0.18);
    let tint = vec3<f32>(param_or(P_TINT_R, 0.94), param_or(P_TINT_G, 0.97), param_or(P_TINT_B, 1.0));

    // The plate's rounded outline, anti-aliased to the pixel.
    let unit_px = 1.0 / max(fx.px_per_unit, 1e-3);
    let r = max(param_or(P_RADIUS, 0.0), 0.0);
    let q = abs(p - size * 0.5) - (size * 0.5 - vec2<f32>(r));
    let sd = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    let mask = clamp(0.5 - sd / unit_px, 0.0, 1.0);
    if (mask <= 0.0) {
        return vec4<f32>(0.0);
    }

    // Shape of the light in a width-independent frame: x in plate
    // heights, so a wide rack face and a lane face light alike.
    let aspect = size.x / max(size.y, 1.0);

    // Brushed: an upright band where the key reflects (leaning a little
    // with the light), a weaker second one at the right from the fill.
    let lean = (uv.y - 0.5) * 0.06;
    let w1 = 0.11 + 0.1 / aspect;
    let b1 = exp(-pow((uv.x - cx - lean) / w1, 2.0));
    let b2 = exp(-pow((uv.x - (cx + 0.47) + lean) / (w1 * 0.8), 2.0)) * 0.45;
    // A slow swell top to bottom: the light is above.
    let swell = mix(1.0, 0.65, uv.y);
    // The grain: streaks along the brushing, long in x, a unit or two tall;
    // finer than ~2 px they average out (no shimmer when drawn small).
    let fine = smoothstep(0.35, 1.2, fx.px_per_unit);
    let s1 = plate_noise(vec2<f32>(p.x * 0.012, p.y * 1.3));
    let s2 = plate_noise(vec2<f32>(p.x * 0.004 + 17.0, p.y * 0.37));
    let streak = mix(1.0, 0.35 + 1.3 * mix(s2, s1, 0.6 * fine), clamp(0.8 * grain_k, 0.0, 1.0));
    let band = (b1 + b2) * swell * streak;

    // Painted: a satin lobe from the upper left, and a softbox reflection —
    // a soft-edged window, sharper the glossier the paint.
    let dl = vec2<f32>((uv.x - 0.24) * aspect / 3.0, uv.y + 0.15);
    let satin = exp(-dot(dl, dl) / (0.18 + 0.5 * rough)) * 0.55;
    let edge = 0.02 + 0.2 * rough;
    let box_x = smoothstep(0.08 - edge, 0.08 + edge, uv.x) * (1.0 - smoothstep(0.42 - edge, 0.42 + edge, uv.x));
    let box_y = 1.0 - smoothstep(0.38 - edge * 1.5, 0.38 + edge * 1.5, uv.y + (uv.x - 0.25) * 0.35);
    let softbox = box_x * box_y * (0.5 - 0.42 * rough);
    let paint = satin + softbox;

    var light = mix(paint, band * 0.85, brushed) * strength;

    // The edges: the top and left catch the light, the foot falls away.
    let e = 1.6;
    let top = (1.0 - smoothstep(0.0, e, p.y)) * 0.55 + (1.0 - smoothstep(0.0, e, p.x)) * 0.3;
    light += top * strength;
    let bottom = 1.0 - smoothstep(0.0, e, size.y - p.y);
    let shade = smoothstep(0.45, 1.0, uv.y) * foot + bottom * 0.35;

    let a_l = clamp(light, 0.0, 0.85);
    let a_s = clamp(shade, 0.0, 0.6) * (1.0 - a_l);
    // Light over shade: premultiplied, the light tinted.
    return vec4<f32>(tint * a_l, a_l + a_s) * mask;
}

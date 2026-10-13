// ── shared by the visualizer built-ins (`viz.*`, `eq.glow`) ─────────────
// Ported from processor's `fts_audio_ui::shader::PRELUDE`: the colour rules
// every panel shader uses. The visualizers draw their own picture from their
// inputs and ignore the source (their subtree is the vector fallback).

// A brighter version of a colour that is MORE of that colour, not less: the
// hue is kept and deepened while the value rises. `k` above 1 overdrives;
// the tone map picks up the rolloff.
fn hotter(c: vec3<f32>, k: f32) -> vec3<f32> {
    let peak = max(max(c.r, c.g), c.b);
    if (peak <= 0.0001) { return c; }
    let hue = c / peak;
    let deep = mix(hue, hue * hue, clamp(k, 0.0, 1.0) * 0.55);
    return deep * peak * (1.0 + k * 1.35);
}

// Reinhard on luminance rather than per channel, so a very bright blue
// stays blue.
fn tonemap(c: vec3<f32>) -> vec3<f32> {
    let l = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    if (l <= 0.0001) { return c; }
    return c * ((l / (1.0 + l)) / l);
}

// A panel shader's result (premultiplied `rgb`, `a`), as it lands on screen
// in signal's rig. processor's `ShaderSurface` renders it with
// `ALPHA_BLENDING` into a cleared target (texel `rgb·a`, `a`), and vello
// composites that texture as STRAIGHT alpha (`register_texture` declares
// `ImageAlphaType::Alpha`), multiplying by `a` once more: what reaches the
// screen is `rgb·a²` over the ground at `1 − a`. Every partly covered pixel
// — a glow's falloff, a haze, a fill — is that much darker than the shader's
// own maths says; this is the picture the rig shows, so it is the one drawn.
fn viz_out(c: vec4<f32>) -> vec4<f32> {
    let a = clamp(c.a, 0.0, 1.0);
    return vec4<f32>(c.rgb * a * a, a);
}

// processor's `lane::beats` — the beat grid its time lanes draw as 1 px
// geometry over the shader — over `c` (premultiplied): a line per beat
// across `window` s at panel x `px_x` of `width`, every fourth (a bar)
// brighter and 1.5 px; none past 64 lines.
fn viz_beats_over(c: vec4<f32>, px_x: f32, width: f32, window: f32, beat: f32, lit: bool) -> vec4<f32> {
    if (beat <= 0.0 || window <= 0.0) { return c; }
    let count = ceil(window / beat);
    if (count > 64.0) { return c; }
    let k = max(round(px_x / width * window / beat), 1.0);
    if (k > count || k * beat > window) { return c; }
    let x = k * beat / window * width;
    let bar = (u32(k) % 4u) == 0u;
    let lw = select(1.0, 1.5, bar);
    let cover = clamp(lw * 0.5 + 0.5 - abs(px_x - x), 0.0, 1.0);
    var a = 0.07;
    if (lit) { a = select(0.22, 0.46, bar); }
    let ga = cover * a;
    let g = vec3<f32>(148.0, 163.0, 184.0) / 255.0 * ga;
    return vec4<f32>(g + c.rgb * (1.0 - ga), ga + c.a * (1.0 - ga));
}

// The lane colour: params `tint_r/g/b` (0..1), else `fallback`.
fn viz_tint(fallback: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(param_or(P_TINT_R, fallback.r), param_or(P_TINT_G, fallback.g), param_or(P_TINT_B, fallback.b));
}

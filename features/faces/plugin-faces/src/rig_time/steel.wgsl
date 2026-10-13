// steel — the Plate's sheet as brushed steel that catches the light. The
// sheet's own paint (its broad light and dark bands) stays the picture;
// over it, the brushing — fine streaks along the sheet, a row of pixels
// each, slowly varying along it — and the highlights stretched across the
// brushing the way brushed metal stretches them: where the paint is
// already light it brightens toward a cool white, in streaks. A light
// from the upper left: the top of the sheet a little brighter. Reads no
// clock: the sheet renders once.
//
// params: strength (0.8) — the highlights; amount (0.5) — the brushing.

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let strength = clamp(param_or(P_STRENGTH, 0.8), 0.0, 2.0);
    let brushing = clamp(param_or(P_AMOUNT, 0.5), 0.0, 1.0);
    let c = sample_src(uv);
    if (c.a <= 0.0) {
        return c;
    }
    let size = node_px();
    let px = uv * size;
    // Brushing: each row its own grain, drifting in runs along the sheet.
    let row = floor(px.y);
    let run = floor(px.x / 23.0 + hash11(row * 0.71) * 7.0);
    let n = 0.6 * hash11(row * 1.37 + 3.1) + 0.4 * hash11(row * 0.53 + run * 13.7);
    let grain = (n - 0.5) * 0.14 * brushing;
    // The paint's own light, straight colour.
    let rgb = c.rgb / c.a;
    let lum = luminance(rgb);
    // Anisotropic highlight: the light bands flare, streaked by the grain.
    let flare = pow(clamp((lum - 0.5) / 0.5, 0.0, 1.0), 2.0);
    // And the window's light itself, reflected: two soft upright streaks
    // (brushed metal stretches a highlight across its grain), broken up
    // row by row.
    let a1 = (uv.x - 0.2) / 0.05;
    let a2 = (uv.x - 0.6) / 0.11;
    let glint = exp(-a1 * a1) + 0.6 * exp(-a2 * a2);
    let top = 0.75 + 0.25 * (1.0 - uv.y) * (1.0 - 0.5 * uv.x);
    let spec = (flare * 0.16 + glint * 0.2) * top * (0.4 + 1.2 * n) * strength;
    // The dark bands deepen a touch: more contrast, as polished metal has.
    let deep = mix(rgb, rgb * rgb * 1.2, 0.3 * strength);
    let out = clamp(deep * (1.0 + grain) + vec3<f32>(0.93, 0.96, 1.0) * spec, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(out * c.a, c.a);
}

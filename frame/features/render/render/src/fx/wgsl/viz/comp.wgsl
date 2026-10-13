// `viz.comp` — the compressor, as light. Ported from processor's
// `comp-ui/src/viz.wgsl`: same maths, same colours; the uniform block is
// filled from frame's inputs instead of a hand-packed buffer.
//
// Inputs: IN_THRESHOLD (dB), IN_RATIO, IN_KNEE (dB), IN_IN_DB (input level,
// dBFS — the ball), IN_GR_DB (gain reduction, dB positive), IN_INPUT and
// IN_GR (traces 0..1, oldest → newest), IN_ON (0/1, default 1),
// IN_GRABBABLE (0/1, default 0). Params: `tint_r/g/b` (the lane colour,
// default processor's input grey), `p0` = the dB range (60).

const TAU: f32 = 6.28318530718;

struct Comp {
    // width, height, seconds, on
    frame: vec4<f32>,
    // threshold_db, ratio, knee_db, range_db
    curve: vec4<f32>,
    // in_db, gr_db, trace_len, grabbable
    meter: vec4<f32>,
    // the lane's colour; w unused
    color: vec4<f32>,
};

var<private> u: Comp;

fn comp_load() {
    let size = node_px();
    u.frame = vec4<f32>(size, fx.time_s, input_or(IN_ON, 1.0));
    u.curve = vec4<f32>(input_or(IN_THRESHOLD, -18.0), max(input_or(IN_RATIO, 4.0), 1.0), input_or(IN_KNEE, 6.0), param_or(P_P0, 60.0));
    u.meter = vec4<f32>(input_or(IN_IN_DB, -120.0), input_or(IN_GR_DB, 0.0), f32(input_len(IN_INPUT)), input_or(IN_GRABBABLE, 0.0));
    u.color = vec4<f32>(viz_tint(vec3<f32>(228.0, 228.0, 231.0) / 255.0), 1.0);
}

// One trace, sampled across the panel with its neighbours blended
// (smoothstep between samples, so it reads as a body, not a polyline).
fn trace_at(slot: u32, x: f32) -> f32 {
    let count = input_len(slot);
    if (count < 2u) { return 0.0; }
    let pos = clamp(x, 0.0, 1.0) * f32(count - 1u);
    let i = u32(floor(pos));
    let j = min(i + 1u, count - 1u);
    return mix(input_raw(slot, i), input_raw(slot, j), smoothstep(0.0, 1.0, fract(pos)));
}

fn input_level_at(x: f32) -> f32 {
    return trace_at(IN_INPUT, x);
}

fn gr_at(x: f32) -> f32 {
    return trace_at(IN_GR, x);
}

// The soft-knee transfer function — the same maths as `comp.transfer`.
fn transfer(input_db: f32) -> f32 {
    let threshold = u.curve.x;
    let ratio = max(u.curve.y, 1.0);
    let knee = u.curve.z;
    let slope = 1.0 - 1.0 / ratio;
    let half_knee = knee * 0.5;
    if (knee > 0.001 && abs(input_db - threshold) < half_knee) {
        let x = input_db - threshold + half_knee;
        return input_db - slope * x * x / (2.0 * knee);
    }
    if (input_db <= threshold) { return input_db; }
    return input_db - slope * (input_db - threshold);
}

// dB (0 at the top … −range at the bottom) → 0..=1 down the panel.
fn db_to_v(db: f32) -> f32 {
    return clamp(-db / max(u.curve.w, 1.0), 0.0, 1.0);
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    comp_load();
    let t = u.frame.z;
    let lit = u.frame.w > 0.5;

    var tint = u.color.rgb;
    if (!lit) { tint = vec3<f32>(0.30, 0.30, 0.34); }
    let hot = hotter(tint, 0.5);
    var cut = vec3<f32>(0.94, 0.35, 0.35);
    if (!lit) { cut = vec3<f32>(0.34, 0.28, 0.30); }

    var rgb = vec3<f32>(0.0);
    var alpha = 0.0;

    // ── The input, rising from the floor ────────────────────────────────
    let lvl = input_level_at(uv.x);
    let top = 1.0 - lvl;
    let d_in = uv.y - top;
    let aa_in = max(fwidth(d_in), 0.0008);
    let below = smoothstep(-aa_in, aa_in, d_in);
    let depth_below = max(uv.y - top, 0.0);
    let falloff = exp(-depth_below * 3.4);
    let body = below * (0.16 + 0.62 * lvl) * (0.22 + 0.78 * falloff);
    let edge = exp(-abs(uv.y - top) * 150.0) * (0.35 + 0.65 * lvl);
    let spill = exp(-max(top - uv.y, 0.0) * (30.0 - 14.0 * lvl)) * 0.26 * lvl;
    rgb += tint * (body + edge * 1.35 + spill);
    alpha += body * 0.80 + edge * 0.85 + spill * 0.6;

    // ── The gain reduction, hanging from the ceiling ────────────────────
    let red = gr_at(uv.x);
    let d_gr = uv.y - red;
    let aa_gr = max(fwidth(d_gr), 0.0008);
    if (red > 0.001) {
        let inside = 1.0 - smoothstep(-aa_gr, aa_gr, d_gr);
        let gr_falloff = exp(-max(red - uv.y, 0.0) * 6.0);
        let gr_body = inside * (0.22 + 0.80 * red) * (0.25 + 0.75 * gr_falloff);
        let gr_edge = exp(-abs(uv.y - red) * 110.0) * (0.5 + 0.8 * red);
        rgb += cut * (gr_body * 1.15 + gr_edge * 1.5);
        alpha += gr_body * 0.75 + gr_edge * 0.85;
    }

    // ── The transfer curve ──────────────────────────────────────────────
    let range = max(u.curve.w, 1.0);
    let in_db = (uv.x - 1.0) * range;
    let out_v = db_to_v(transfer(in_db));
    let d_curve = uv.y - out_v;
    let slope = max(fwidth(out_v), 1e-5);
    let dist = abs(d_curve) / sqrt(1.0 + slope * slope * u.frame.x * u.frame.x
        / max(u.frame.y * u.frame.y, 1.0)) * u.frame.y;
    let line = exp(-(dist * dist) / 3.0);
    let bloom = exp(-dist / 9.0) * 0.34;
    rgb += hot * (line * 0.95 + bloom);
    alpha += line * 0.9 + bloom * 0.45;

    let knee_top = db_to_v(transfer(u.curve.x));
    if (uv.y < knee_top) {
        let work = (knee_top - uv.y) / max(knee_top, 1e-3);
        rgb += cut * work * 0.10;
        alpha += work * 0.09;
    }

    // ── The threshold, when it can be taken ─────────────────────────────
    if (u.meter.w > 0.5) {
        let ty = db_to_v(u.curve.x);
        let near = exp(-abs(uv.y - ty) * 26.0);
        let pulse = 0.78 + 0.22 * sin(t * TAU * 0.9);
        let handle = near * 0.30 * pulse;
        rgb += hotter(cut, 0.35) * handle;
        alpha += handle * 0.7;
    }

    // ── The ball ────────────────────────────────────────────────────────
    if (lit) {
        let bx = clamp(u.meter.x / range + 1.0, 0.0, 1.0);
        let by = db_to_v(transfer(u.meter.x));
        let d = length((vec2<f32>(bx, by) - uv) * vec2<f32>(u.frame.x, u.frame.y));
        let work = clamp(u.meter.y / 12.0, 0.0, 1.0);
        let r = 3.0 + 2.0 * work * (1.0 + 0.12 * sin(t * TAU * 0.8));
        let core = exp(-(d * d) / (r * r * 0.6));
        let halo = exp(-d / (r * 4.0)) * 0.5;
        rgb += mix(hot, cut, work) * (core + halo);
        alpha += core * 0.95 + halo * 0.4;
    }

    rgb = tonemap(rgb * 2.2);
    alpha = clamp(alpha, 0.0, 0.94);
    return viz_out(vec4<f32>(rgb * alpha, alpha));
}

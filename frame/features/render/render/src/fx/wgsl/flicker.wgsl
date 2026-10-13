// flicker — tube-lamp brightness jitter over time (the node's emission
// flickers with it; see `flicker_level` on the CPU side, which mirrors this).
// params: strength (0.5) — depth, speed (10) — jitter rate, Hz.

fn effect(uv: vec2<f32>) -> vec4<f32> {
    let depth = clamp(param_or(P_STRENGTH, 0.5), 0.0, 1.0);
    let speed = max(param_or(P_SPEED, 10.0), 0.0);
    let x = fx.time_s * speed;
    let i = floor(x);
    let f = x - i;
    let s = f * f * (3.0 - 2.0 * f);
    let noise = mix(hash11(i), hash11(i + 1.0), s);
    var dropout = 0.0;
    if (hash11(floor(x * 0.37) + 7.0) > 0.9) {
        dropout = 0.6;
    }
    let level = clamp(1.0 - depth * (noise * 0.5 + dropout), 0.0, 1.0);
    let c = sample_src(uv);
    return vec4<f32>(c.rgb * level, c.a);
}

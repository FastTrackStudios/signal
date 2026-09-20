// The modulation panels, on the GPU.
//
// Every engine here works by interference — copies of a signal against
// itself, displaced in time or pitch, reinforcing and cancelling. So the
// picture is interference: fields beating against each other at the engine's
// own rate and depth, which is both what the effect does and the reason it
// looks the way it does.
//
// `u.frame` is [width, height, seconds, engine]; `u.params` is
// [rate_hz, depth, mix, engaged]; `u.color` is the group's colour.

const CHORUS:  f32 = 0.0;
const FLANGER: f32 = 1.0;
const PHASER:  f32 = 2.0;
const TREMOLO: f32 = 3.0;
const VIBRATO: f32 = 4.0;
const ROTARY:  f32 = 5.0;

const TAU: f32 = 6.28318530718;

// A cheap smooth noise, for the grain that keeps a flat field from looking
// like a gradient.
fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(hash(i + vec2<f32>(0.0, 0.0)), hash(i + vec2<f32>(1.0, 0.0)), u.x),
        mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x),
        u.y
    );
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let t = u.frame.z;
    let engine = u.frame.w;
    let rate = max(u.params.x, 0.01);
    let depth = clamp(u.params.y, 0.0, 1.0);
    let engaged = u.params.w;

    // The LFO every engine is driven by, so they share a heartbeat even when
    // they draw differently.
    let phase = t * rate;
    let lfo = sin(phase * TAU);

    // Distance from the centre line, which is where most of these live.
    let mid = abs(uv.y - 0.5) * 2.0;

    var field = 0.0;

    if (engine < CHORUS + 0.5) {
        // Three detuned voices beating against each other. The beat
        // frequency IS the chorus.
        var v = 0.0;
        for (var i = 0; i < 3; i = i + 1) {
            let detune = (f32(i) - 1.0) * depth * 0.35;
            let k = 26.0 + detune * 8.0;
            v = v + sin((uv.x * k) * TAU + phase * TAU + f32(i) * 2.1);
        }
        field = v / 3.0;
        // Bands where the voices agree, which is what you hear thicken.
        field = field * (1.0 - mid * 0.7);
    } else if (engine < FLANGER + 0.5) {
        // A comb whose teeth slide: cos of a frequency that sweeps.
        let sweep = (lfo * 0.5 + 0.5) * depth;
        let teeth = 6.0 + 26.0 * sweep;
        field = cos(uv.x * teeth * TAU) * (1.0 - mid * 0.5);
        // Sharpen toward notches, which is where a flanger lives.
        field = sign(field) * pow(abs(field), 0.45);
    } else if (engine < PHASER + 0.5) {
        // Four allpass notches travelling through the band.
        var v = 1.0;
        for (var i = 0; i < 4; i = i + 1) {
            let centre = fract(0.12 + 0.2 * f32(i) + (lfo * 0.5 + 0.5) * 0.3);
            let d = (uv.x - centre) / (0.035 + 0.02 * depth);
            v = v - depth * exp(-d * d);
        }
        field = (v - 0.5) * 2.0 * (1.0 - mid * 0.6);
    } else if (engine < TREMOLO + 0.5) {
        // Amplitude, pumping: the carrier's envelope is the LFO.
        let env = mix(1.0 - depth, 1.0, sin((uv.x - phase) * TAU) * 0.5 + 0.5);
        let carrier = sin(uv.x * 40.0 * TAU);
        field = carrier * env;
        field = field * step(mid, env);
    } else if (engine < VIBRATO + 0.5) {
        // Pitch, bending: the argument is modulated, so cycles bunch.
        let bend = depth * 0.6 * sin((uv.x * 2.0 - phase) * TAU);
        field = sin((uv.x * 30.0 + bend * 8.0) * TAU) * (1.0 - mid * 0.5);
    } else {
        // A horn going round: distance to a point on an orbit, and the
        // doppler that comes with it.
        let a = phase * TAU;
        let c = vec2<f32>(0.5 + cos(a) * 0.32 * (0.5 + depth * 0.5), 0.5 + sin(a) * 0.30);
        let d = distance(uv, c);
        let near = sin(a) * 0.5 + 0.5;
        field = exp(-d * d * (70.0 - 30.0 * near)) * 2.0 - 0.4;
        // The room it throws into.
        field = field + exp(-abs(d - 0.22) * 14.0) * 0.35 * near;
    }

    // Grain, so a flat region reads as material rather than as a fill.
    let grain = (noise(uv * vec2<f32>(u.frame.x, u.frame.y) * 0.08 + t * 0.3) - 0.5) * 0.10;
    var lit = clamp(abs(field) + grain, 0.0, 1.5);

    // A bypassed engine keeps its shape and loses its light — "off" and
    // "not configured" must not look the same.
    lit = lit * mix(0.16, 1.0, engaged);

    // Colour: the group's hue, lifted toward white where the field is
    // strongest, so the bright parts read as energy rather than as a
    // different colour arriving.
    let base = u.color.rgb;
    let hot = mix(base, vec3<f32>(1.0), clamp(lit - 0.55, 0.0, 1.0) * 0.85);
    let rgb = hot * clamp(lit, 0.0, 1.0);

    // Alpha carries the field too, so the panel's own ground shows through
    // the quiet parts instead of being covered by a dark rectangle.
    let alpha = clamp(lit * 0.82, 0.0, 0.92);
    return vec4<f32>(rgb * alpha, alpha);
}

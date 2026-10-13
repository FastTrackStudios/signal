// The effects stage's own passes: blit-over, premultiply/downsample,
// separable Gaussian blur, additive bloom. One fullscreen triangle each.

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: VsOut;
    o.pos = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    o.uv = uv;
    return o;
}

struct Pass {
    dir: vec2<f32>,
    sigma: f32,
    taps: i32,
    step: u32,
    premul: u32,
    gain: f32,
    // Bloom kept inside the emitters themselves (0..1): they're already lit,
    // so most of the light shows as a halo around them.
    inside: f32,
}

@group(0) @binding(0) var<uniform> u: Pass;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;
// The emitters' own coverage (bloom only).
@group(0) @binding(3) var mask: texture_2d<f32>;

fn load(p: vec2<i32>) -> vec4<f32> {
    let size = vec2<i32>(textureDimensions(src));
    if (any(p < vec2<i32>(0)) || any(p >= size)) {
        return vec4<f32>(0.0);
    }
    return textureLoad(src, p, 0);
}

fn premultiply(c: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(c.rgb * c.a, c.a);
}

// A straight-alpha layer (vello's output), premultiplied, for blending
// "over" (One, OneMinusSrcAlpha).
@fragment
fn fs_blit(in: VsOut) -> @location(0) vec4<f32> {
    let c = premultiply(load(vec2<i32>(in.pos.xy)));
    // `premul` = 1: faded by `gain` (a layer's opacity, applied here
    // rather than rendered in).
    return select(c, c * u.gain, u.premul == 1u);
}

// Box-average `step`×`step` source texels into one; premultiply first when
// the source is straight alpha.
@fragment
fn fs_down(in: VsOut) -> @location(0) vec4<f32> {
    let base = vec2<i32>(in.pos.xy) * i32(u.step);
    var acc = vec4<f32>(0.0);
    for (var y = 0; y < i32(u.step); y++) {
        for (var x = 0; x < i32(u.step); x++) {
            var c = load(base + vec2<i32>(x, y));
            if (u.premul != 0u) {
                c = premultiply(c);
            }
            acc += c;
        }
    }
    return acc / f32(u.step * u.step);
}

// One axis of a Gaussian (sigma in texels); outside the texture is empty.
@fragment
fn fs_blur(in: VsOut) -> @location(0) vec4<f32> {
    let p = vec2<i32>(in.pos.xy);
    let d = vec2<i32>(u.dir);
    let k = 1.0 / (2.0 * u.sigma * u.sigma);
    var acc = vec4<f32>(0.0);
    var wsum = 0.0;
    for (var i = -u.taps; i <= u.taps; i++) {
        let w = exp(-f32(i * i) * k);
        acc += load(p + d * i) * w;
        wsum += w;
    }
    return acc / wsum;
}

// Add blurred light over the frame (blend One + One on colour), mostly
// outside the emitters.
@fragment
fn fs_bloom(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(src, samp, in.uv, 0.0);
    let covered = textureSampleLevel(mask, samp, in.uv, 0.0).a;
    return vec4<f32>(c.rgb * u.gain * mix(1.0, u.inside, covered), 0.0);
}

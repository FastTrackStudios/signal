// knob.3d — a 3D knob rendered per pixel over the node's box: a signed
// distance field of cap, skirt and insert, sphere-traced from a camera a
// little in front of the panel (an oblique view: the panel stays square,
// what stands off it leans up and shows its front wall), turned by the
// control's value under a light that stays put.
//
// The box is the knob's footprint plus room for its shadow: the knob's
// outer radius (skirt, or cap without one) is `radius` × half the box's
// shorter side, centred. The subtree (the vector knob) is not sampled: the
// shader replaces it; renderers without the stage draw it instead.
//
// Params (named slots; see CONTRACT.md "knob.3d"):
//   strength  specular strength (1)          speed     anisotropy 0..1 (0)
//   amount    metalness 0..1 (0)             scale     top-edge bevel radius (0.07)
//   angle     sweep, degrees (300)           elevation view tilt, degrees (30)
//   softness  roughness 0..1 (0.38)          radius    footprint / half box (0.66)
//   tint_r/g/b cap colour (sRGB 0..1)         tint_a    clearcoat 0..1 (0)
//   p0 cap profile  0 flat · 1 domed · 2 fluted · 3 knurled · 4 chicken head
//                   5 teardrop pointer cap · 6 wing · 7 stepped
//                   8 fluted body + teardrop insert
//   p1 ribs (flutes or knurl teeth; 0 = the profile's default)
//   p2 skirt        0 none · 1 plain · 2 flared · 3 knurled · 4 metal · 5 dished
//   p3 pointer      0 none · 1 line · 2 dot · 3 line down the side too · 4 engraved groove
// Inputs (optional `Const` arrays; a missing value takes its default):
//   IN_SHAPE   [cap radius .72, height .5, dome 0, rib depth .045,
//               knurl band 1, step radius .5, step height .14, diamond 0,
//               skirt teeth 48]
//   IN_SKIRT   [r, g, b, metal, roughness, clearcoat, radius 1, height .16]
//   IN_INSERT  [r, g, b, metal, roughness, anisotropy, size .62, height .09]
//   IN_POINTER [r, g, b, width .075, inner .3, outer .92]
//   IN_LIGHT   [shadow 1, occlusion 1, fill 1, rim 1, key 1]
//   IN_VALUE   the position 0..1 (bind a param with a map) — else `value`

const K_PI: f32 = 3.14159265;
const K_TAU: f32 = 6.28318531;

const ID_CAP: f32 = 0.0;
const ID_SKIRT: f32 = 1.0;
const ID_INSERT: f32 = 2.0;

// Toward the kit's key light: upper left (azimuth −35°, kit angles), 50° up.
// World axes: x right, y down the panel (toward the viewer), z off it.
const KEY: vec3<f32> = vec3<f32>(-0.3687, -0.5267, 0.7660);
const FILL: vec3<f32> = vec3<f32>(0.5514, 0.4511, 0.7017);
const RIM: vec3<f32> = vec3<f32>(0.1504, -0.8521, 0.5013);

struct Mat {
    base: vec3<f32>,
    metal: f32,
    rough: f32,
    coat: f32,
    aniso: f32,
}

struct Knob {
    profile: i32,
    skirt: i32,
    pointer: i32,
    ribs: f32,
    cap_r: f32,
    height: f32,
    dome: f32,
    bevel: f32,
    rib_depth: f32,
    band: f32,
    step_r: f32,
    step_h: f32,
    diamond: f32,
    skirt_teeth: f32,
    skirt_r: f32,
    skirt_h: f32,
    insert_size: f32,
    insert_h: f32,
    // Pointer: width, inner and outer radius (cap radii).
    ptr_w: f32,
    ptr_in: f32,
    ptr_out: f32,
    // Rotation: cos, sin of the turn.
    rc: f32,
    rs: f32,
    // Oblique view: screen y = y − tilt·z.
    tilt: f32,
    // World units per target pixel.
    pw: f32,
    // Rib detail kept (0 = averaged away when ribs are under ~2 px).
    rib_lod: f32,
    knurl_lod: f32,
    top: f32,
    // The cap's bounding radius.
    bound: f32,
    // Anything reads the angle round the shaft (flutes, knurl).
    ribbed: bool,
}

var<private> K: Knob;
// The angle round the shaft at the point `map` is looking at, in turns.
var<private> TN: f32;

fn kin(slot: u32, k: u32, fallback: f32) -> f32 {
    if (input_len(slot) > k) {
        return input_raw(slot, k);
    }
    return fallback;
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    return pow(max(c, vec3<f32>(0.0)), vec3<f32>(2.2));
}

// ── 2D / profile distance helpers ────────────────────────────────────────

fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

fn smax(a: f32, b: f32, k: f32) -> f32 {
    return -smin(-a, -b, k);
}

// A 2D shape `d2` stood between z = 0 and `h` with its top edge rounded by
// `r` (the bottom sits on whatever is under it).
fn stand(d2: f32, z: f32, h: f32, r: f32) -> f32 {
    let w = vec2<f32>(d2 + r, z - h + r);
    let top = min(max(w.x, w.y), 0.0) + length(max(w, vec2<f32>(0.0))) - r;
    return max(top, -z);
}

// iq's uneven capsule, from the origin (radius r1) up +y to (0, h) (r2).
fn uneven_capsule(q: vec2<f32>, r1: f32, r2: f32, h: f32) -> f32 {
    let p = vec2<f32>(abs(q.x), q.y);
    let b = (r1 - r2) / h;
    let a = sqrt(max(1.0 - b * b, 0.0));
    let k = dot(p, vec2<f32>(-b, a));
    if (k < 0.0) {
        return length(p) - r1;
    }
    if (k > a * h) {
        return length(p - vec2<f32>(0.0, h)) - r2;
    }
    return dot(p, vec2<f32>(a, b)) - r1;
}

// iq's trapezoid in (r, z): bottom half-width r1, top r2, half-height he.
fn trapezoid(q: vec2<f32>, r1: f32, r2: f32, he: f32) -> f32 {
    let k1 = vec2<f32>(r2, he);
    let k2 = vec2<f32>(r2 - r1, 2.0 * he);
    let p = vec2<f32>(abs(q.x), q.y);
    let ca = vec2<f32>(p.x - min(p.x, select(r2, r1, p.y < 0.0)), abs(p.y) - he);
    let cb = p - k1 + k2 * clamp(dot(k1 - p, k2) / dot(k2, k2), 0.0, 1.0);
    let s = select(1.0, -1.0, cb.x < 0.0 && ca.y < 0.0);
    return s * sqrt(min(dot(ca, ca), dot(cb, cb)));
}

fn box2(p: vec2<f32>, b: vec2<f32>) -> f32 {
    let d = abs(p) - b;
    return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}

// Knob-local (turning) coordinates of a world point on the panel plane.
fn turn(p: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(K.rc * p.x + K.rs * p.y, -K.rs * p.x + K.rc * p.y);
}

// The angle round the shaft, 0 at the pointer, clockwise, in turns.
fn turns(l: vec2<f32>) -> f32 {
    return atan2(l.x, -l.y) / K_TAU;
}

// How far ribs cut into a radius at local `l`, height `z`: flutes (round
// scallops), straight knurl (V teeth) or diamond knurl.
fn flute_cut(l: vec2<f32>, n: f32, depth: f32) -> f32 {
    let u = fract(TN * n) - 0.5;
    // A round scallop centred on u = 0, its lands flat between.
    let w = 0.36;
    let s = clamp(abs(u) / w, 0.0, 1.0);
    return depth * sqrt(1.0 - s * s) * K.rib_lod;
}

fn knurl_cut(l: vec2<f32>, z: f32, n: f32, depth: f32) -> f32 {
    let t = TN * n;
    let lod = K.knurl_lod;
    var tri = abs(fract(t) - 0.5) * 2.0;
    if (K.diamond > 0.5) {
        // Two helices crossing: pyramids.
        let pitch = z * n / (K_TAU * max(K.cap_r, 0.1));
        let a = abs(fract(t + pitch) - 0.5) * 2.0;
        let b = abs(fract(t - pitch) - 0.5) * 2.0;
        tri = max(a, b);
    }
    return depth * mix(0.5, 1.0 - tri, lod);
}

// ── the knob ─────────────────────────────────────────────────────────────

fn skirt_d(l: vec2<f32>, z: f32) -> f32 {
    let r = length(l);
    let h = K.skirt_h;
    let rs = K.skirt_r;
    if (K.skirt == 2 || K.skirt == 5) {
        // Flared: wide at the panel, narrowing to the cap.
        let top_r = mix(K.cap_r * 1.04, rs, 0.25);
        var d = trapezoid(vec2<f32>(r, z - h * 0.5), rs, top_r, h * 0.5) - 0.012;
        if (K.skirt == 5) {
            // A dish: a raised lip round the edge.
            d = smin(d, stand(r - rs, z, h * 0.62, 0.03), 0.03);
        }
        return max(d, -z);
    }
    var cut = 0.0;
    if (K.skirt == 3) {
        cut = knurl_cut(l, z, K.skirt_teeth, 0.035);
    }
    return stand(r + cut - rs, z, h, 0.028);
}

// The cap: a lathe-turned body (flat, domed, fluted, knurled, stepped) or
// a moulded one (chicken head, wing), and the teardrop's insert.
fn cap_d(l: vec2<f32>, z: f32) -> vec2<f32> {
    let r = length(l);
    let H = K.height;
    let b = K.bevel;
    let p = K.profile;
    var d = 1e3;
    var id = ID_CAP;
    if (p == 4) {
        // Chicken head: a round boss behind the shaft and a long beak, one
        // moulding: its top slopes down toward the tip and is gabled along
        // the beak, so the ridge catches the light.
        let boss_r = K.cap_r * 0.55;
        let back = boss_r * 0.4;
        let q = vec2<f32>(l.x, -(l.y - back));
        let d2 = uneven_capsule(q, boss_r, 0.075, 0.92 + back - 0.075);
        let along = clamp(-l.y / 0.92, 0.0, 1.0);
        let beak_top = mix(H, H * 0.7, along) - 0.32 * sqrt(l.x * l.x + 0.002) * (0.4 + along);
        let w = vec2<f32>(d2 + b, z - beak_top + b);
        d = max(min(max(w.x, w.y), 0.0) + length(max(w, vec2<f32>(0.0))) - b, -z);
    } else if (p == 5) {
        // A teardrop cap: round behind the shaft, its point the index.
        let q = vec2<f32>(l.x, -(l.y - K.cap_r * 0.1));
        let d2 = uneven_capsule(q, K.cap_r * 0.8, 0.07, 0.96 + K.cap_r * 0.1 - 0.07);
        let top = H + K.dome * (1.0 - clamp(dot(l, l) / (K.cap_r * K.cap_r), 0.0, 1.0)) - 0.12 * max(-l.y - K.cap_r * 0.6, 0.0);
        let w = vec2<f32>(d2 + b, z - top + b);
        d = max(min(max(w.x, w.y), 0.0) + length(max(w, vec2<f32>(0.0))) - b, -z);
    } else if (p == 6) {
        // Wing: a dome crossed by a fin that runs out to the pointer.
        let rr = K.cap_r * 0.86;
        let dome = stand(r - rr, z, H * 0.62, rr * 0.6);
        let y = l.y;
        let fin_top = H * (1.0 - 0.55 * pow(clamp((y + 0.1) / 1.0, -1.0, 1.0), 2.0));
        let d2 = box2(vec2<f32>(l.x, y + 0.2), vec2<f32>(0.1, 0.78));
        let w = vec2<f32>(d2 + 0.06, z - fin_top + 0.06);
        let fin = max(min(max(w.x, w.y), 0.0) + length(max(w, vec2<f32>(0.0))) - 0.06, -z);
        d = smin(dome, fin, 0.07);
    } else {
        var cut = 0.0;
        if (p == 2 || p == 8) {
            cut = flute_cut(l, K.ribs, K.rib_depth);
        } else if (p == 3) {
            let band = smoothstep(H * K.band + 0.03, H * K.band - 0.03, z);
            cut = knurl_cut(l, z, K.ribs, K.rib_depth) * band;
        }
        d = stand(r + cut - K.cap_r, z, H, b);
        if (K.dome > 0.001) {
            // A spherical crown `dome` above the top edge.
            let a = K.cap_r - b * 0.5;
            let rs = (a * a + K.dome * K.dome) / (2.0 * K.dome);
            let zc = H + K.dome - rs;
            let sphere = length(vec3<f32>(l, z - zc)) - rs;
            d = smax(stand(r + cut - K.cap_r, z, H + K.dome, b), sphere, b * 0.8);
        }
        if (p == 7) {
            d = smin(d, stand(r - K.step_r, z, H + K.step_h, b * 0.8), 0.015);
        }
        if (p == 8) {
            // The teardrop insert, standing proud of the body's top.
            let s = K.insert_size;
            let q = vec2<f32>(l.x, -(l.y - s * 0.12));
            let d2 = uneven_capsule(q, s * 0.62, s * 0.12, s * 0.95);
            let top = H + K.insert_h - 0.05 * dot(l, l);
            let w = vec2<f32>(d2 + 0.05, z - top + 0.05);
            let ins = max(min(max(w.x, w.y), 0.0) + length(max(w, vec2<f32>(0.0))) - 0.05, H * 0.5 - z);
            if (ins < d) {
                id = ID_INSERT;
            }
            d = min(d, ins);
        }
    }
    if (K.pointer == 4) {
        // An engraved groove along the pointer.
        let g = box2(l - vec2<f32>(0.0, -(K.ptr_in + K.ptr_out) * 0.5 * K.cap_r), vec2<f32>(K.ptr_w * 0.5, (K.ptr_out - K.ptr_in) * 0.5 * K.cap_r));
        d = max(d, -max(g, K.top - 0.035 - z));
    }
    return vec2<f32>(d, id);
}

// The scene: (distance, part id).
fn map(p: vec3<f32>) -> vec2<f32> {
    let l = turn(p.xy);
    if (K.ribbed) {
        TN = turns(l);
    }
    // Far from a part, its bounding cylinder is distance enough.
    let r = length(l);
    let far = 0.08;
    var res = vec2<f32>(max(max(r - K.bound, p.z - K.top), -p.z), ID_CAP);
    if (res.x < far) {
        res = cap_d(l, p.z);
    }
    if (K.skirt > 0) {
        var s = max(max(r - K.skirt_r, p.z - K.skirt_h - 0.02), -p.z);
        if (s < far) {
            s = skirt_d(l, p.z);
        }
        if (s < res.x) {
            res = vec2<f32>(s, ID_SKIRT);
        }
    }
    return res;
}

fn normal_at(p: vec3<f32>) -> vec3<f32> {
    let e = max(K.pw * 0.5, 0.002);
    let k0 = vec3<f32>(1.0, -1.0, -1.0);
    let k1 = vec3<f32>(-1.0, -1.0, 1.0);
    let k2 = vec3<f32>(-1.0, 1.0, -1.0);
    let k3 = vec3<f32>(1.0, 1.0, 1.0);
    return normalize(k0 * map(p + k0 * e).x + k1 * map(p + k1 * e).x + k2 * map(p + k2 * e).x + k3 * map(p + k3 * e).x);
}

// ── shading ──────────────────────────────────────────────────────────────

fn fresnel(f0: vec3<f32>, c: f32) -> vec3<f32> {
    return f0 + (vec3<f32>(1.0) - f0) * pow(1.0 - clamp(c, 0.0, 1.0), 5.0);
}

// GGX (anisotropic when `ax != ay`), Smith visibility, times N·L.
fn ggx(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, t: vec3<f32>, ax: f32, ay: f32) -> f32 {
    let nl = dot(n, l);
    if (nl <= 0.0) {
        return 0.0;
    }
    let h = normalize(l + v);
    let bt = cross(n, t);
    let th = dot(t, h) / ax;
    let bh = dot(bt, h) / ay;
    let nh = dot(n, h);
    let q = th * th + bh * bh + nh * nh;
    let dd = 1.0 / (K_PI * ax * ay * q * q);
    let nv = max(dot(n, v), 1e-3);
    let a = sqrt(ax * ay);
    let vis = 0.5 / (nl * (nv * (1.0 - a) + a) + nv * (nl * (1.0 - a) + a));
    return dd * vis * nl;
}

// The studio the knob reflects: a dark room, a bright softbox where the key
// light is, a strip light behind, the panel below.
fn env(rd: vec3<f32>, rough: f32) -> vec3<f32> {
    let s = 0.012 + 0.5 * rough * rough;
    let gain = 0.07 / (0.07 + s);
    // Ceiling brighter toward the key's side, falling to the horizon.
    let side = 0.5 + 0.5 * dot(normalize(rd.xy + vec2<f32>(1e-4, 0.0)), normalize(KEY.xy));
    let up = smoothstep(-0.15, 0.95, rd.z);
    // The floor is the lit panel round the knob.
    let room = mix(vec3<f32>(0.16, 0.155, 0.15), vec3<f32>(0.16, 0.165, 0.175) * (0.35 + side), up);
    let soft = exp(-(1.0 - dot(rd, KEY)) / (s + 0.035)) * 2.4 * gain;
    let strip = exp(-(1.0 - dot(rd, RIM)) / (s * 1.6 + 0.01)) * 0.9 * gain;
    let fill = exp(-(1.0 - dot(rd, FILL)) / (s * 2.5 + 0.1)) * 0.25;
    return room + vec3<f32>(1.0, 0.98, 0.95) * soft + vec3<f32>(0.9, 0.95, 1.0) * (strip + fill);
}

fn soft_shadow(p: vec3<f32>) -> f32 {
    var res = 1.0;
    var t = 0.04;
    for (var i = 0; i < 3; i++) {
        let h = map(p + KEY * t).x;
        res = min(res, 7.0 * h / t);
        t += clamp(h, 0.08, 0.3);
        if (res < 0.02 || t > 1.2) {
            break;
        }
    }
    return clamp(res, 0.0, 1.0);
}

fn occlusion(p: vec3<f32>, n: vec3<f32>) -> f32 {
    // One tap: how much of a small step off the surface is inside it.
    let a = map(p + n * 0.06).x;
    return clamp(0.35 + 0.65 * a / 0.06, 0.0, 1.0);
}

// The parts' materials, the pointer's colour and the lights, read once per
// pixel up front (the reads overlap; spread through the shading they'd
// each wait on memory).
var<private> MAT_CAP: Mat;
var<private> MAT_SKIRT: Mat;
var<private> MAT_INSERT: Mat;
var<private> PTR_COLOR: vec3<f32>;
// shadow, occlusion, fill, rim; key
var<private> LIGHTS: vec4<f32>;
var<private> KEY_I: f32;

fn materials_load() {
    MAT_SKIRT.base = to_linear(vec3<f32>(kin(IN_SKIRT, 0u, 0.06), kin(IN_SKIRT, 1u, 0.06), kin(IN_SKIRT, 2u, 0.065)));
    MAT_SKIRT.metal = kin(IN_SKIRT, 3u, select(0.0, 1.0, K.skirt == 4));
    MAT_SKIRT.rough = clamp(kin(IN_SKIRT, 4u, select(0.4, 0.22, K.skirt == 4)), 0.03, 1.0);
    MAT_SKIRT.coat = kin(IN_SKIRT, 5u, 0.0);
    MAT_SKIRT.aniso = 0.0;
    MAT_INSERT.base = to_linear(vec3<f32>(kin(IN_INSERT, 0u, 0.82), kin(IN_INSERT, 1u, 0.83), kin(IN_INSERT, 2u, 0.85)));
    MAT_INSERT.metal = kin(IN_INSERT, 3u, 1.0);
    MAT_INSERT.rough = clamp(kin(IN_INSERT, 4u, 0.2), 0.03, 1.0);
    MAT_INSERT.coat = 0.0;
    MAT_INSERT.aniso = kin(IN_INSERT, 5u, 0.0);
    MAT_CAP.base = to_linear(vec3<f32>(param_or(P_TINT_R, 0.1), param_or(P_TINT_G, 0.1), param_or(P_TINT_B, 0.105)));
    MAT_CAP.metal = clamp(param_or(P_AMOUNT, 0.0), 0.0, 1.0);
    MAT_CAP.rough = clamp(param_or(P_SOFTNESS, 0.38), 0.03, 1.0);
    MAT_CAP.coat = clamp(param_or(P_TINT_A, 0.0), 0.0, 1.0);
    MAT_CAP.aniso = clamp(param_or(P_SPEED, 0.0), 0.0, 1.0);
    PTR_COLOR = to_linear(vec3<f32>(kin(IN_POINTER, 0u, 0.95), kin(IN_POINTER, 1u, 0.94), kin(IN_POINTER, 2u, 0.9)));
    LIGHTS = vec4<f32>(kin(IN_LIGHT, 0u, 1.0), clamp(kin(IN_LIGHT, 1u, 1.0), 0.0, 1.0), kin(IN_LIGHT, 2u, 1.0), kin(IN_LIGHT, 3u, 1.0));
    KEY_I = kin(IN_LIGHT, 4u, 1.0);
}

fn material_of(id: f32) -> Mat {
    if (id == ID_SKIRT) {
        return MAT_SKIRT;
    }
    if (id == ID_INSERT) {
        return MAT_INSERT;
    }
    return MAT_CAP;
}

fn shade(p: vec3<f32>, n_in: vec3<f32>, v: vec3<f32>, id: f32) -> vec3<f32> {
    var m = material_of(id);
    var n = n_in;
    let l = turn(p.xy);
    let r = length(p.xy);
    // Circumferential tangent: spun metal is turned round the shaft.
    var t = normalize(cross(vec3<f32>(0.0, 0.0, 1.0), n) + vec3<f32>(1e-4, 0.0, 0.0));
    if (r > 1e-3) {
        let tc = vec3<f32>(-p.y / r, p.x / r, 0.0);
        t = normalize(tc - n * dot(tc, n));
    }
    if (m.aniso > 0.01 && n.z > 0.6) {
        // Machining rings: faint grooves in the normal and the albedo.
        let ring = sin(r * 260.0) * 0.5 + 0.5;
        let fade = clamp(1.6 - K.pw * 260.0 / K_TAU, 0.0, 1.0);
        m.base *= 1.0 - 0.1 * ring * fade * m.aniso;
        n = normalize(n + vec3<f32>(p.xy / max(r, 1e-3), 0.0) * (ring - 0.5) * 0.05 * fade);
    }
    // The pointer: paint on the cap (or its insert).
    var paint = 0.0;
    if (K.pointer > 0 && id != ID_SKIRT) {
        let len_in = K.ptr_in * K.cap_r;
        let len_out = K.ptr_out * K.cap_r;
        var d = 1.0;
        if (K.pointer == 2) {
            d = length(l - vec2<f32>(0.0, -(len_in + len_out) * 0.5)) - K.ptr_w * 0.7;
        } else {
            d = box2(l - vec2<f32>(0.0, -(len_in + len_out) * 0.5), vec2<f32>(K.ptr_w * 0.5, (len_out - len_in) * 0.5));
        }
        let on_top = select(smoothstep(0.1, 0.45, n.z), 1.0, K.pointer == 3 || K.pointer == 4);
        paint = clamp(0.5 - d / max(K.pw, 1e-4), 0.0, 1.0) * on_top;
        if (K.pointer == 4) {
            paint *= step(p.z, K.top - 0.01);
        }
    }
    if (paint > 0.0) {
        let pc = PTR_COLOR;
        m.base = mix(m.base, pc, paint);
        m.metal = mix(m.metal, 0.0, paint);
        m.rough = mix(m.rough, 0.45, paint);
        m.aniso = mix(m.aniso, 0.0, paint);
    }

    let key_i = KEY_I * 1.2;
    let fill_i = LIGHTS.z * 0.35;
    let rim_i = LIGHTS.w * 0.9;
    let spec_k = max(param_or(P_STRENGTH, 1.0), 0.0);
    let ao = mix(1.0, occlusion(p, n), LIGHTS.y);
    var sh = 1.0;
    // Only what sits under the cap is shadowed by it (the skirt, a wing's
    // dome under its fin); the cap's own far side is dark by N·L.
    if (id == ID_SKIRT || (K.profile == 6 && p.z < K.height * 0.66)) {
        sh = soft_shadow(p + n * 0.01);
    }

    let nv = max(dot(n, v), 1e-3);
    let f0 = mix(vec3<f32>(0.04), m.base, m.metal);
    let diff = m.base * (1.0 - m.metal);
    // Diffuse: key (shadowed), fill, a little sky.
    var col = diff * (max(dot(n, KEY), 0.0) * key_i * sh + max(dot(n, FILL), 0.0) * fill_i * vec3<f32>(0.9, 0.95, 1.05) + (0.1 + 0.12 * n.z) * ao);
    // Specular: the three lights (area-ish: roughness floored) …
    let a0 = max(m.rough * m.rough, 0.02);
    let ax = a0 * (1.0 - 0.85 * m.aniso);
    let ay = min(a0 * (1.0 + 2.5 * m.aniso) + 0.1 * m.aniso, 1.0);
    let fk = fresnel(f0, dot(normalize(KEY + v), v));
    var spec = fk * ggx(n, v, KEY, t, ax, ay) * key_i * sh;
    spec += fresnel(f0, dot(normalize(FILL + v), v)) * ggx(n, v, FILL, t, ax, ay) * fill_i;
    spec += fresnel(f0, dot(normalize(RIM + v), v)) * ggx(n, v, RIM, t, ax, ay) * rim_i * sh;
    // … and the room (softbox, strip), faded where the knob hides it.
    let rd = reflect(-v, n);
    let fe = fresnel(f0, nv);
    spec = spec * 0.35 + fe * env(rd, m.rough + m.aniso * 0.25) * mix(ao, 1.0, 0.3) * mix(0.35, 1.0, sh);
    col += spec * spec_k;
    // Clearcoat: a second, sharp lobe over the base (bakelite, lacquer).
    if (m.coat > 0.0) {
        let fc = fresnel(vec3<f32>(0.04), nv).x * m.coat;
        let cs = env(rd, 0.06) * fc + ggx(n, v, KEY, t, 0.02, 0.02) * 0.04 * m.coat * key_i * sh;
        col = col * (1.0 - fc) + cs * spec_k;
    }
    return col;
}

// Filmic shoulder, then sRGB-ish gamma.
fn display(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    let knee = vec3<f32>(0.75);
    let over = max(x - knee, vec3<f32>(0.0));
    let y = min(x, knee) + (vec3<f32>(1.0) - knee) * (vec3<f32>(1.0) - exp(-over / (vec3<f32>(1.0) - knee)));
    return pow(y, vec3<f32>(1.0 / 2.2));
}

// ── the effect ───────────────────────────────────────────────────────────

fn knob_load() {
    let profile = i32(round(param_or(P_P0, 0.0)));
    K.profile = profile;
    K.skirt = i32(round(param_or(P_P2, 0.0)));
    K.pointer = i32(round(param_or(P_P3, 1.0)));
    var ribs = param_or(P_P1, 0.0);
    if (ribs < 1.0) {
        ribs = select(select(8.0, 60.0, profile == 3), 12.0, profile == 8);
    }
    K.ribs = ribs;
    K.cap_r = kin(IN_SHAPE, 0u, 0.72);
    K.height = kin(IN_SHAPE, 1u, 0.5);
    K.dome = kin(IN_SHAPE, 2u, select(0.0, 0.16, profile == 1));
    K.rib_depth = kin(IN_SHAPE, 3u, select(0.045, 0.02, profile == 3));
    K.band = kin(IN_SHAPE, 4u, 1.0);
    K.step_r = kin(IN_SHAPE, 5u, 0.5);
    K.step_h = kin(IN_SHAPE, 6u, 0.14);
    K.diamond = kin(IN_SHAPE, 7u, 0.0);
    K.skirt_teeth = kin(IN_SHAPE, 8u, 48.0);
    K.bevel = clamp(param_or(P_SCALE, 0.07), 0.005, 0.4);
    K.skirt_r = kin(IN_SKIRT, 6u, 1.0);
    K.skirt_h = kin(IN_SKIRT, 7u, 0.16);
    K.insert_size = kin(IN_INSERT, 6u, 0.62);
    K.insert_h = kin(IN_INSERT, 7u, 0.09);
    K.ptr_w = kin(IN_POINTER, 3u, 0.075);
    K.ptr_in = kin(IN_POINTER, 4u, 0.3);
    K.ptr_out = kin(IN_POINTER, 5u, 0.92);
    var top = K.height + K.dome;
    if (profile == 7) {
        top = max(top, K.height + K.step_h);
    }
    if (profile == 8) {
        top = max(top, K.height + K.insert_h);
    }
    K.top = top;
    var bound = K.cap_r + 0.01;
    if (profile == 4 || profile == 5 || profile == 6) {
        bound = 1.0;
    } else if (profile == 8) {
        bound = max(bound, K.insert_size * 1.02 + 0.02);
    }
    K.bound = bound;
    K.ribbed = profile == 2 || profile == 3 || profile == 8 || K.skirt == 3;
    let v = clamp(input_or(IN_VALUE, fx.value), 0.0, 1.0);
    let a = (v - 0.5) * radians(param_or(P_ANGLE, 300.0));
    K.rc = cos(a);
    K.rs = sin(a);
    K.tilt = tan(radians(clamp(param_or(P_ELEVATION, 30.0), 0.0, 50.0)));
}

fn effect(uv: vec2<f32>) -> vec4<f32> {
    knob_load();
    // Knob units: the footprint's radius is 1.
    let box = fx.node_size;
    let half = 0.5 * min(box.x, box.y);
    let scale = half * clamp(param_or(P_RADIUS, 0.66), 0.1, 1.0);
    let s = (uv - vec2<f32>(0.5)) * box / scale;
    // Derivatives first, in uniform control flow.
    let fw = fwidth(s);
    K.pw = max(max(fw.x, fw.y), 1e-4);
    // Ribs narrower than ~2.5 px average away (no shimmer when small).
    K.rib_lod = clamp((K_TAU * K.cap_r / (K.ribs * K.pw) - 2.5) / 3.0, 0.0, 1.0);
    K.knurl_lod = clamp((K_TAU * K.cap_r / (K.ribs * K.pw) - 3.0) / 4.0, 0.0, 1.0);
    materials_load();
    let shadow_k = LIGHTS.x;

    // Panel: contact shadow and occlusion, analytic — a disc (the base)
    // swept away from the light up the knob's height, blurred with distance.
    let foot = select(K.cap_r, max(K.skirt_r, K.cap_r), K.skirt > 0);
    let away = -KEY.xy / KEY.z;
    let seg = away * K.top * 0.7;
    let hproj = clamp(dot(s, seg) / dot(seg, seg), 0.0, 1.0);
    let sd = length(s - seg * hproj) - mix(foot, K.cap_r * 0.9, hproj);
    let cast_sh = 1.0 - smoothstep(-0.04 - 0.08 * hproj, 0.06 + 0.22 * hproj, sd);
    let contact = exp(-max(length(s) - foot, 0.0) * 14.0);
    // Faded out before the box edge, which would cut it square.
    let edge = min(min(uv.x, uv.y), min(1.0 - uv.x, 1.0 - uv.y));
    let shadow_a = clamp((cast_sh * 0.45 + contact * 0.45) * shadow_k, 0.0, 0.92) * smoothstep(0.0, 0.14, edge);

    // The ray: from above the knob, down and back into the panel.
    let dir = normalize(vec3<f32>(0.0, -K.tilt, -1.0));
    let zmax = K.top + 0.05;
    // Outside the knob's lean up the screen nothing is hit.
    let lean = vec2<f32>(0.0, clamp(s.y, -K.tilt * zmax, 0.0));
    if (length(s - lean) > max(K.bound, select(0.0, K.skirt_r, K.skirt > 0)) + 0.04) {
        return vec4<f32>(0.0, 0.0, 0.0, shadow_a);
    }
    var ro = vec3<f32>(s.x, s.y + K.tilt * zmax, zmax);
    var t = 0.0;
    var hit = false;
    var dmin = 1e3;
    var tmin = 0.0;
    var tmax = zmax * length(vec2<f32>(K.tilt, 1.0));
    var id = ID_CAP;
    // Skip to where the ray enters the knob's bounding cylinder.
    let reach = max(K.bound, select(0.0, K.skirt_r, K.skirt > 0)) + 0.02;
    let yb = sqrt(max(reach * reach - s.x * s.x, 0.0));
    if (K.tilt > 1e-3 && ro.y > yb) {
        t = (ro.y - yb) * length(vec2<f32>(K.tilt, 1.0)) / K.tilt;
    }
    let slant = length(vec2<f32>(K.tilt, 1.0));
    // … and stop where it leaves it (behind the knob).
    if (K.tilt > 1e-3) {
        tmax = min(tmax, (ro.y + yb) * slant / K.tilt);
    }
    for (var i = 0; i < 16; i++) {
        let p = ro + dir * t;
        let m = map(p);
        if (m.x < dmin) {
            dmin = m.x;
            tmin = t;
            id = m.y;
        }
        if (m.x < K.pw * 0.4) {
            hit = true;
            break;
        }
        // The panel is a surface too: a ray reaching it is done (it would
        // otherwise creep along the knob's foot).
        if (p.z < K.pw * 0.4) {
            break;
        }
        t += max(min(m.x, p.z * slant) * 0.9, K.pw * 0.3);
        if (t > tmax) {
            break;
        }
    }
    // Coverage from the closest approach (anti-aliased silhouette).
    let cover = select(clamp(0.5 - dmin / K.pw, 0.0, 1.0), 1.0, hit);
    if (cover <= 0.0) {
        return vec4<f32>(0.0, 0.0, 0.0, shadow_a);
    }
    let p = ro + dir * tmin;
    let n = normal_at(p);
    // Shade as a camera a few radii away sees it (the view changes across
    // a flat top, so reflections sweep over it).
    let cam = vec3<f32>(0.0, 7.0 * K.tilt, 7.0) * inverseSqrt(1.0 + K.tilt * K.tilt);
    let rgb = display(shade(p, n, normalize(cam - p), id));
    let a = cover + (1.0 - cover) * shadow_a;
    return vec4<f32>(rgb * cover, a);
}

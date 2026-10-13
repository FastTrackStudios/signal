// The reverb's decay, as light.
//
// The vector pass draws the same decay as geometry; what a shader adds is the
// part that is about a field of energy rather than an outline — the density of
// early reflections as actual grain, the tail as a haze that thins rather than
// a curve that descends, the bloom where a reflection lands. Those are
// per-pixel, and as geometry each one costs a shape.
//
// Layout of `u` mirrors `ReverbUniforms` in viz.rs.
//
// The beat grid is NOT here, for the same reason as the delay's: a 1 px ruled
// line is the one thing the vector pass does better, and the grid is what the
// tail's length is read against.

const TAU: f32 = 6.28318530718;

// The families, in `family_index`'s order. The two must agree.
const ROOM:        f32 = 0.0;
const HALL:        f32 = 1.0;
const PLATE:       f32 = 2.0;
const SPRING:      f32 = 3.0;
const AMBIENT:     f32 = 4.0;
const RANDOM:      f32 = 5.0;
const SPECIAL:     f32 = 6.0;
const CONVOLUTION: f32 = 7.0;

struct Reverb {
    // width, height, seconds, on
    frame: vec4<f32>,
    // decay_s, density, predelay_s, mix
    params: vec4<f32>,
    // beat_s, window_s, damp, unused
    time: vec4<f32>,
    // the lane's colour; w unused
    color: vec4<f32>,
    // family, unused, unused, unused
    style: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Reverb;

fn is_family(f: f32) -> bool {
    return abs(u.style.x - f) < 0.5;
}

// How many discrete reflections the front of the tail shows before it stops
// being countable.
//
// This is the first thing that tells the machines apart. A room is a handful
// of close walls and you hear every one; a plate has no walls at all and is
// dense from the first millisecond; a spring is a few big bounces. Drawing
// them all with the same early field made every algorithm look alike, which
// is the one thing a per-algorithm picture must not do.
fn early_count(density: f32) -> f32 {
    if (is_family(ROOM)) { return 6.0 + 8.0 * density; }
    if (is_family(HALL)) { return 8.0 + 20.0 * density; }
    // A plate has no walls and a spring's early field IS its chirp; an
    // ambient's is velvet's grains. None of them has discrete arrivals to
    // draw, and drawing some anyway is what made every family look alike.
    if (is_family(PLATE)) { return 0.0; }
    if (is_family(SPRING)) { return 0.0; }
    if (is_family(AMBIENT)) { return 0.0; }
    if (is_family(SPECIAL)) { return 16.0 + 16.0 * density; }
    if (is_family(CONVOLUTION)) { return 20.0 + 28.0 * density; }
    return 6.0 + 26.0 * density;
}

/// The most arrivals the early field draws.
const MAX_EARLY: u32 = 48u;

// WHERE the early reflections land, which is the family's fingerprint.
//
// Every family used one hash for placement and differed only in how many
// arrivals it drew — so a room, a hall, a gated reverb and a convolution all
// showed the same scatter at different densities, and the eye reads the same
// scatter as the same machine. The pattern has to differ, because the pattern
// is the thing: a room's returns come off parallel walls in pairs, a hall's
// thicken as the space diffuses, a measured space arrives in clusters with
// real gaps between them, and a random space's do not stay where they were.
//
// `k` is 0..1 across the early span, `py` is -1..1 across the lane.
fn early_field(k: f32, py: f32, density: f32, t: f32) -> f32 {
    let total = early_count(density);
    if (total < 0.5) { return 0.0; }
    let n = u32(min(total, f32(MAX_EARLY)));

    var acc = 0.0;
    for (var i = 0u; i < MAX_EARLY; i = i + 1u) {
        if (i >= n) { break; }
        let fi = f32(i);
        let fn_ = f32(n);
        let s1 = hash11(fi + 1.0);
        let s2 = hash11(fi + 17.0);

        var kx = (fi + 0.5) / fn_;
        var y = mix(-0.8, 0.8, s2);
        var amp = 0.45 + 0.55 * s1;
        var w = 0.030;

        if (is_family(ROOM)) {
            // Off parallel walls, in pairs, evenly: a box returns the same
            // slap from two sides and it is regular enough to count.
            kx = (fi + 0.5) / fn_;
            y = select(0.62, -0.62, (i % 2u) == 0u) * (0.65 + 0.35 * s1);
            amp = (1.0 - kx * 0.45) * (0.7 + 0.3 * s1);
            w = 0.022;
        } else if (is_family(HALL)) {
            // Sparse at first and thickening: a big space takes time to
            // diffuse, and watching it fill in IS the size.
            kx = pow((fi + 0.5) / fn_, 0.62);
            amp = (0.35 + 0.65 * s1) * (0.55 + 0.45 * kx);
            w = 0.034;
        } else if (is_family(RANDOM)) {
            // They do not stay where they were. Position AND height drift,
            // which is the one family whose character is that it is never
            // the same twice.
            kx = clamp((fi + 0.5) / fn_ + sin(t * 0.55 + fi * 1.7) * 0.045, 0.0, 1.0);
            y = sin(t * 0.7 + fi * 2.3) * 0.82;
            amp = 0.45 + 0.55 * s1;
            w = 0.032;
        } else if (is_family(SPECIAL)) {
            // A lattice. Gated reverbs are dense and machine-made, and the
            // regularity is why the cliff at the end reads as a decision
            // rather than as an ending.
            kx = (fi + 0.5) / fn_;
            y = (f32(i % 5u) / 4.0 - 0.5) * 1.7;
            amp = 0.85;
            w = 0.018;
        } else if (is_family(CONVOLUTION)) {
            // Clusters with real gaps between them. A measured space is not
            // evenly spaced — that is exactly how a recording differs from
            // a synthesis, and it is the only honest way to draw one.
            let group = floor(fi / 5.0);
            let within = fi - group * 5.0;
            kx = clamp(group * 0.235 + within * 0.028 + s1 * 0.018, 0.0, 1.0);
            y = mix(-0.92, 0.92, s2);
            amp = (0.3 + 0.7 * s1) * (1.0 - kx * 0.35);
            w = 0.016;
        }

        let dk = (k - kx) / w;
        let dy = (py - y) / 0.26;
        acc = acc + exp(-(dk * dk + dy * dy)) * amp;
    }
    return acc;
}

// How far into the window the early field reaches, as a fraction.
fn early_reach() -> f32 {
    if (is_family(ROOM)) { return 0.16; }
    if (is_family(SPRING)) { return 0.42; }
    if (is_family(CONVOLUTION)) { return 0.55; }
    return 0.28;
}

// The envelope's shape, on the dB axis, 0..=1 at `k` of the way through the
// decay. Straight is the default; the families that are not straight are the
// ones whose whole character is the shape.
fn shape(k: f32) -> f32 {
    let lin = clamp(1.0 - k, 0.0, 1.0);
    if (is_family(AMBIENT)) {
        // No walls: it SWELLS before it goes. A bloom that started at full
        // level and fell would be indistinguishable from a hall.
        let rise = smoothstep(0.0, 0.22, k);
        return clamp(rise * lin * 1.6, 0.0, 1.0);
    }
    if (is_family(SPECIAL)) {
        // Gated and non-linear: it holds, then stops. The cliff is the
        // effect — a gate that faded out would not be a gate.
        return select(0.0, 1.0, k < 0.78) * clamp(1.0 - smoothstep(0.74, 0.80, k), 0.0, 1.0);
    }
    if (is_family(PLATE)) {
        // Steel is bright and quick at the end.
        return pow(lin, 1.35);
    }
    return lin;
}

// A cheap stable hash, for reflection placement. Deterministic in x, so the
// reflections stand still instead of crawling: where they ARE is the density,
// and a field that shimmered position would be saying something untrue.
fn hash11(p: f32) -> f32 {
    return fract(sin(p * 127.1) * 43758.5453);
}

// The tail's height at `t` seconds, 0..=1 — on the dB axis, not the linear
// one.
//
// RT60 is the definition: 60 dB down at `decay` seconds. Plotting the LINEAR
// amplitude of that is a curve which has visually vanished by a fifth of the
// way across (−14 dB is already 0.2 of full scale), so nine tenths of the
// panel draws a tail that is still audibly there as nothing at all. Every
// other meter in this rack reads in dB for the same reason.
//
// On the dB axis the same envelope is a straight fall from full height to the
// floor at exactly `decay` — so the SLOPE is the RT60, which is the number
// the picture exists to show, and two reverbs set the same still look the
// same.
fn envelope_at(t: f32, decay: f32) -> f32 {
    let predelay = max(u.params.z, 0.0);
    if (t < predelay) { return 0.0; }
    let since = t - predelay;
    return shape(since / max(decay, 1e-3));
}

// The whole tail: the low end, which damping does not touch.
fn envelope(t: f32) -> f32 {
    return envelope_at(t, max(u.params.x, 1e-3));
}

// The BRIGHT part of the tail — the top of the spectrum, which damping eats.
//
// A room and a hall set to the same RT60 draw the identical wedge, and the
// thing that actually tells them apart is how fast the highs go. Two nested
// envelopes say it in one picture: a bright core that dies early inside a
// dimmer body that lasts, and the gap between them IS the damping. At zero
// damping they coincide and the whole tail is bright, which is what an
// undamped reverb sounds like.
fn envelope_high(t: f32) -> f32 {
    // A gate defines its own end. Damping the highs early would cut the
    // bright core before the cliff and put TWO edges on the panel, which
    // reads as a reverb that stops twice — the opposite of what a gate is.
    if (is_family(SPECIAL)) { return envelope(t); }
    let damp = clamp(u.time.z, 0.0, 1.0);
    let decay = max(u.params.x, 1e-3) * (1.0 - 0.78 * damp);
    return envelope_at(t, decay);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let px = uv * vec2<f32>(u.frame.x, u.frame.y);
    let t = u.frame.z;
    let lit = u.frame.w > 0.5;

    let window = max(u.time.y, 1e-3);
    let predelay = max(u.params.z, 0.0);
    let density = clamp(u.params.y, 0.0, 1.0);
    let mid = 0.5;

    var tint = u.color.rgb;
    if (!lit) { tint = vec3<f32>(0.32, 0.32, 0.36); }
    // Only a little toward white. At 0.55 the tail came out grey — the
    // body covers most of the panel, so whatever colour it is IS the colour
    // of the effect, and a purple reverb whose tail is white is not one.
    let hot = hotter(tint, 0.34);

    var rgb = vec3<f32>(0.0);
    var alpha = 0.0;

    // Seconds at this column.
    let at = uv.x * window;
    let env = envelope(at);
    let env_hi = envelope_high(at);
    // Distance from the centre line as a fraction of the half-height.
    let off = abs(uv.y - mid) * 2.0;

    // ── Before anything arrives ─────────────────────────────────────────
    //
    // Pre-delay is silence, and silence should look like silence: the gap
    // before the first reflection is the single clearest thing separating a
    // room from a hall, so it is drawn as an actually empty lane.
    let gap = 1.0 - step(predelay, at);

    // ── The lane itself ─────────────────────────────────────────────────
    //
    // The panel is a PURPLE field, not a black box with purple marks on it.
    // See the delay's note: a near-black ground made every time-effect lane
    // look the same, and the ground is where a panel says what it is before
    // anything is drawn on it.
    //
    // Pre-delay stays darker than the rest, because silence before the first
    // reflection is the clearest thing separating a room from a hall and it
    // should look like silence — but it is not black, or the gap reads as a
    // panel that has not loaded.
    // Violet-led rather than near-neutral: the ground is most of the panel,
    // so where it sits between black and the lane's colour is what the eye
    // actually calls the panel's colour.
    let base = mix(vec3<f32>(0.036, 0.016, 0.068), tint, 0.16);
    let along = mix(1.30, 0.78, uv.x);
    let across = mix(1.12, 0.70, clamp(off, 0.0, 1.0));
    let ground_rgb = base * along * across * (1.0 - gap * 0.45);
    let edge_y = 1.0 - smoothstep(0.80, 1.0, off) * 0.45;
    let ground_a = 0.95 * edge_y;

    // A random space's delay lines walk, so its envelope is never quite the
    // same twice — the tail breathes rather than falling cleanly. This is
    // the family's entire character and it is invisible in a still frame,
    // which is exactly why it has to move.
    var env_w = env;
    if (is_family(RANDOM) && lit) {
        let walk = sin(uv.x * 5.0 - t * 0.9) * sin(uv.x * 2.3 + t * 0.5);
        env_w = clamp(env * (1.0 + walk * 0.22), 0.0, 1.0);
    }

    // ── The tail ────────────────────────────────────────────────────────
    //
    // A body that narrows as it decays, hazed rather than outlined. The eye
    // reads the RATE it closes, which is the RT60 made visible.
    let span = env_w;
    let inside = 1.0 - smoothstep(span * 0.72, span * 1.1, off);
    let body = inside * env_w * 0.34;
    rgb += tint * body;
    alpha += body * 0.75;

    // ── Steel ───────────────────────────────────────────────────────────
    //
    // A plate is a sheet of metal under tension, and what it sounds like is
    // bright and ringing rather than roomy. It has no early reflections to
    // set it apart — that IS its character, density from the first
    // millisecond — so without something else it is the dullest panel here
    // and reads as a hall with the interesting part missing. The shimmer is
    // the modal ringing the sheet actually has.
    if (is_family(PLATE) && lit && at >= predelay) {
        let modes = sin(uv.y * 38.0 + t * 1.3) * sin(uv.x * 19.0 - t * 2.1);
        let ring = max(modes, 0.0) * env * inside * 0.30;
        rgb += hotter(hot, 0.45) * ring;
        alpha += ring * 0.55;
    }

    // The lit rim where the body ends — the decay curve itself, as an edge
    // made of light rather than a stroked path.
    let rim = exp(-abs(off - span) * 17.0) * env_w * 0.8;
    rgb += hot * rim;
    alpha += rim * 0.85;

    // ── The bright core ─────────────────────────────────────────────────
    //
    // The high end of the tail, inside the body: where it stops is where the
    // room goes dull. With damping at zero it reaches the same place as the
    // rim and the tail is bright all the way; wound up, it closes early and
    // leaves a long dim body behind it.
    let span_hi = env_hi;
    let inside_hi = 1.0 - smoothstep(span_hi * 0.66, span_hi * 1.05, off);
    let core = inside_hi * env_hi * 0.26;
    let core_rim = exp(-abs(off - span_hi) * 20.0) * env_hi * 0.65;
    rgb += hotter(hot, 0.22) * (core + core_rim);
    alpha += core * 0.6 + core_rim * 0.7;

    // ── The early reflections ───────────────────────────────────────────
    //
    // Discrete arrivals near the front, packing tighter as density rises
    // until they stop being countable and become the wash. That transition
    // IS density, and it is why this is grain rather than a number.
    let early_span = predelay + max(u.params.x, 1e-3) * early_reach();
    if (at < early_span && at >= predelay) {
        let k = (at - predelay) / max(early_span - predelay, 1e-4);
        // Early reflections thin out towards the wash rather than stopping at
        // a hard line — the transition from countable to uncountable IS the
        // density, and an edge would draw a boundary the ear does not hear.
        let into_wash = 1.0 - smoothstep(0.58, 1.0, k);
        let field = early_field(k, (uv.y - mid) * 2.0, density, t);
        let spark = field * env * into_wash;
        rgb += hot * spark * 1.25;
        alpha += spark * 0.80;
    }

    // ── The wash ────────────────────────────────────────────────────────
    //
    // Past the early field the reflections are too dense to count, so they
    // are drawn as a moving grain instead. This is the only part that moves,
    // and it moves because a tail is not a still object.
    if (lit && at >= predelay) {
        // Two drifting fields rather than a product of two pixel-rate sines.
        // The sines striped the panel vertically at any real resolution, which
        // reads as static sitting on top of the reverb rather than as the
        // reverb itself — and static does not decay, so it fought the one
        // thing the picture is saying.
        //
        // Slow, wide, and crossing at an angle: close to still where the tail
        // is quiet, alive where it is not.
        let q = vec2<f32>(uv.x * 7.0, (uv.y - mid) * 5.0);
        let a = sin(q.x * 1.7 - t * 0.55 + q.y * 0.9);
        let b = sin(q.x * 1.1 + t * 0.37 - q.y * 1.6);
        let field = (a * b) * 0.5 + 0.5;
        // Density is how BROKEN the wash is: a sparse reverb still has
        // separable reflections this far out, a dense one is smooth.
        let broken = mix(smoothstep(0.35, 0.95, field), field, density);
        let wash = broken * env * inside * 0.30 * (0.4 + 0.6 * density)
            * (0.45 + 0.55 * env_hi);
        rgb += mix(tint, hot, 0.35) * wash;
        alpha += wash * 0.55;
    }

    // ── Flutter ─────────────────────────────────────────────────────────
    //
    // A small room has parallel walls close together, and what that sounds
    // like is a flutter: the same slap coming back at a fixed short spacing,
    // over and over, long after the reflections should have smeared into a
    // tail. It is the single artefact that says "this is a box" rather than
    // "this is a big space", and without it a room drew the same picture as
    // a hall with fewer early reflections — which the eye reads as the same
    // picture.
    //
    // Evenly spaced, unlike everything else in the early field, because
    // being evenly spaced is the whole complaint.
    if (is_family(ROOM) && lit && at >= predelay) {
        let decay = max(u.params.x, 1e-3);
        // Tighter walls as density rises.
        let spacing = mix(0.085, 0.040, density);
        let k = (at - predelay) / decay;
        let slap = fract(k / spacing);
        let hit = exp(-slap * 14.0) + exp(-(1.0 - slap) * 14.0);
        // Flutter outlives the early field but not the tail.
        let live = exp(-k * 2.2);
        let flutter = hit * live * env * inside * 0.55;
        rgb += hotter(hot, 0.30) * flutter;
        alpha += flutter * 0.65;
    }

    // ── The spring ──────────────────────────────────────────────────────
    //
    // A spring disperses: high frequencies travel the coil faster than low
    // ones, so a single hit arrives as a descending chirp — the "boing" that
    // no other reverb makes. Drawn as chirp streaks sweeping down the panel,
    // which is the one picture that could only be a spring.
    if (is_family(SPRING) && lit && at >= predelay) {
        let since = at - predelay;
        var chirp = 0.0;
        for (var i = 0u; i < 3u; i = i + 1u) {
            let born = f32(i) * 0.14;
            let age = since - born;
            if (age <= 0.0) { continue; }
            // Frequency falls as the bounce travels: y is where the energy
            // has got to, and it slides toward the bottom of the lane.
            let fall = 1.0 - exp(-age * 5.5);
            let y = mix(0.12, 0.88, fall);
            let d = abs(uv.y - y);
            let w = 0.05 + 0.10 * fall;
            chirp = chirp + exp(-(d * d) / (w * w)) * env * exp(-age * 1.6);
        }
        rgb += hotter(hot, 0.25) * chirp * 0.75;
        alpha += chirp * 0.6;
    }

    // ── Velvet ──────────────────────────────────────────────────────────
    //
    // The ambient family's sparse random grains: a cloud made of individual
    // impulses far enough apart to be heard as texture rather than as a
    // wash. Density thins them rather than thickening them, which is the
    // opposite of every other family here.
    if (is_family(AMBIENT) && lit && at >= predelay) {
        let cellx = floor(uv.x * 90.0);
        let celly = floor((uv.y - mid) * 24.0);
        let seed = hash11(cellx * 7.3 + celly * 13.1);
        let alive = step(0.82 - 0.25 * density, seed);
        let grain = alive * exp(-length(vec2<f32>(fract(uv.x * 90.0) - 0.5,
                                                  fract((uv.y - mid) * 24.0) - 0.5)) * 6.0);
        let twinkle = 0.5 + 0.5 * sin(t * 2.2 + seed * TAU);
        let velvet = grain * env * twinkle * 0.55;
        rgb += mix(tint, hot, 0.6) * velvet;
        alpha += velvet * 0.6;
    }

    // ── The arrival ─────────────────────────────────────────────────────
    //
    // A bright mark where the first reflection lands. With the empty gap
    // beside it, this is the pre-delay stated as a picture.
    let arrive = exp(-abs(at - predelay) * 240.0 / window) * (1.0 - off * 0.5);
    rgb += hot * arrive * 0.7;
    alpha += arrive * 0.6;

    // Driven before the tone map. Reinhard on raw contributions crushes
    // everything below 1.0, and 1.0 is brighter than anything this panel
    // produces — so without the drive the whole picture sits in the bottom
    // quarter of the curve and reads as switched off.
    // A gentler drive than the delay's: a delay's marks are thin sticks on
    // an empty lane and need lifting, where a reverb's tail covers most of
    // the panel — driven as hard it saturates into one solid mass and loses
    // the decay shape that is the entire message.
    rgb = tonemap(rgb * 1.5);
    alpha = clamp(alpha, 0.0, 0.92);

    // Marks over ground, composited properly rather than summed, and handed
    // back premultiplied because that is what the compositor expects.
    //
    // The ground stays OUT of the curve above: it is not light the panel is
    // emitting, it is the panel, and running it through the same tone map
    // lifted it until it competed with what was drawn on it — a lane so
    // bright the marks had no contrast left to stand out against.
    let out_a = alpha + ground_a * (1.0 - alpha);
    let out_rgb = rgb * alpha + ground_rgb * ground_a * (1.0 - alpha);
    return vec4<f32>(out_rgb, out_a);
}

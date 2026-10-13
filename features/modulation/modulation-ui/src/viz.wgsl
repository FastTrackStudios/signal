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

// A frequency response, drawn as a curve with a body under it.
//
// `gain` is 0..1 at this column — 1 is untouched, 0 is a full notch. The
// filter engines are drawn this way rather than as a field of brightness
// because a field cannot say HOW MANY notches there are or how deep they go,
// and that is the entire difference between the two of them: a flanger is a
// dense harmonic comb, a phaser is four or six dips wandering through the
// band. Rendered as brightness they were both "vertical banding" and the
// pair was indistinguishable.
fn response_curve(uv: vec2<f32>, gain: f32) -> f32 {
    // 1 near the top of the lane, 0 near the bottom, with a margin so a full
    // notch still has somewhere to be.
    let y = 0.14 + (1.0 - clamp(gain, 0.0, 1.0)) * 0.72;
    let d = uv.y - y;

    // A tight core inside a wider glow, rather than one falloff doing both
    // jobs: the core is what makes the curve readable to a pixel, the glow
    // is what makes it look like light.
    let core = exp(-(d * d) / 0.00035);
    let glow = exp(-abs(d) * 11.0) * 0.34;
    // A body under it, so the curve reads as a filled response and not as a
    // wire — falling away beneath its own edge rather than filling flat. A
    // constant fill is a coloured rectangle with a line on top: it says the
    // response reached here and nothing about the shape, and on a short lane
    // it is most of the lane.
    let below = max(d, 0.0);
    let under = smoothstep(0.0, 0.014, d) * (0.09 + 0.24 * gain) * exp(-below * 2.6);
    // And a haze above it, so a peak looks like one.
    let halo = exp(-max(-d, 0.0) * 9.0) * 0.18 * gain;
    return core * 1.25 + glow + under + halo;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let t = u.frame.z;
    let engine = u.frame.w;
    let rate = max(u.params.x, 0.01);
    let depth = clamp(u.params.y, 0.0, 1.0);
    // `wet` rather than `mix`: `mix` is a WGSL builtin and shadowing it costs
    // every interpolation in the file.
    let wet_mix = clamp(u.params.z, 0.0, 1.0);
    let engaged = u.params.w;

    // The LFO every engine is driven by, so they share a heartbeat even when
    // they draw differently.
    let phase = t * rate;
    let lfo = sin(phase * TAU);

    // Distance from the centre line, which is where most of these live.
    let mid = abs(uv.y - 0.5) * 2.0;

    var field = 0.0;

    if (engine < CHORUS + 0.5) {
        // A braid.
        //
        // A chorus is copies of one signal, each on its own slowly-moving
        // delay, and what you hear is them pulling apart and closing again —
        // so what it should look like is strands weaving through one
        // another. Drawn as three separate lines it was legible but inert;
        // drawn as a braid the voices actually cross, and every crossing is
        // the thickening you hear, in the place you hear it.
        //
        // Every parameter has something to move:
        //   rate  — how many turns the braid makes across the lane
        //   depth — how far the strands swing apart
        //   mix   — how bright the wet strands are against the dry one
        //
        // One helix in three phases, a third of a turn apart, so the strands
        // are genuinely the same path and genuinely interleave.
        let turns = 0.9 + 2.4 * clamp(rate / 4.0, 0.0, 1.0);
        let swing = 0.055 + depth * 0.32;
        let wet = 0.45 + 0.75 * wet_mix;

        var ys = array<f32, 3>(0.0, 0.0, 0.0);
        var strands = 0.0;
        for (var i = 0; i < 3; i = i + 1) {
            let a = uv.x * turns * TAU - phase * TAU + f32(i) * (TAU / 3.0);
            let y = 0.5 + sin(a) * swing;
            ys[i] = y;
            // Thinner than the old lines, because now there is light between
            // them doing the work the thickness used to do.
            let d = (uv.y - y) / 0.030;
            strands = strands + (exp(-d * d) + exp(-abs(d) * 0.75) * 0.30) * wet;
        }

        // The dry signal, steady down the middle. Without it there is
        // nothing for the wet voices to be diverging FROM, and a braid with
        // no axis is just a knot.
        let dd = (uv.y - 0.5) / 0.016;
        let dry = exp(-dd * dd) * 0.55;

        // Where two strands converge the light piles up. This is the whole
        // point: a crossing is where two copies agree, which is exactly the
        // moment the sound thickens.
        var knots = 0.0;
        for (var i = 0; i < 3; i = i + 1) {
            for (var j = i + 1; j < 3; j = j + 1) {
                let gap = abs(ys[i] - ys[j]);
                let meeting = exp(-gap * 22.0);
                let centre = (ys[i] + ys[j]) * 0.5;
                let k = (uv.y - centre) / 0.075;
                knots = knots + meeting * exp(-k * k) * 0.85;
            }
        }

        // The body the braid encloses, shimmering: the chorused signal has
        // width, and the width is the effect.
        var lo = min(ys[0], min(ys[1], ys[2]));
        var hi = max(ys[0], max(ys[1], ys[2]));
        let inside = smoothstep(lo - 0.02, lo + 0.02, uv.y)
            * (1.0 - smoothstep(hi - 0.02, hi + 0.02, uv.y));
        let shimmer = inside * (0.55 + 0.45 * sin(uv.x * 44.0 + t * 2.6)) * 0.16 * wet_mix;

        field = strands + dry + knots + shimmer;
    } else if (engine < FLANGER + 0.5) {
        // A comb, and the teeth slide.
        //
        // HARMONIC: the notches are integer multiples of one delay, evenly
        // spaced all the way up, and there are a lot of them. That is what a
        // flanger is and what separates it from the phaser beside it — the
        // count and the regularity, both of which a response curve states
        // and a brightness field cannot.
        let sweep = (lfo * 0.5 + 0.5) * depth;
        let teeth = 5.0 + 20.0 * sweep;
        // |cos| is 1 at the peaks and 0 at the notches — the comb itself.
        let comb = abs(cos(uv.x * teeth * 3.14159265));
        // How DEEP the notches go is the wet/dry blend, not the sweep: a
        // comb filter nulls completely at 50/50 and not at all when the wet
        // path is muted. Depth slides the teeth; mix decides whether there
        // are teeth at all.
        let gain = mix(1.0, comb, 0.08 + 0.92 * wet_mix);
        field = response_curve(uv, gain);
    } else if (engine < PHASER + 0.5) {
        // Allpass notches travelling through the band.
        //
        // FEW, and NOT harmonically spaced — that is the whole difference
        // from the comb next door. Six of them, drifting at their own rates,
        // wide enough to see individually.
        let travel = lfo * 0.5 + 0.5;
        var gain = 1.0;
        for (var i = 0; i < 6; i = i + 1) {
            let fi = f32(i);
            // Spacing that widens as it climbs, so no two gaps match and the
            // eye cannot read it as a comb.
            let home = 0.08 + fi * 0.13 + fi * fi * 0.012;
            let centre = fract(home + travel * 0.22);
            let width = 0.030 + 0.022 * depth;
            let d = (uv.x - centre) / width;
            // Same division as the flanger: depth moves the notches, mix
            // decides how deep they cut.
            gain = gain - (0.10 + 0.85 * wet_mix) * exp(-d * d);
        }
        field = response_curve(uv, clamp(gain, 0.0, 1.0));
    } else if (engine < TREMOLO + 0.5) {
        // Amplitude, pumping: the carrier's envelope is the LFO.
        //
        // The envelope floors at a fraction of full height rather than at
        // zero, so the quiet part of the cycle is still a waveform being
        // held down rather than a hole in the panel. A tremolo at full depth
        // is silent for part of its cycle, but silence in a picture is
        // indistinguishable from nothing loaded.
        let swing = sin((uv.x - phase) * TAU) * 0.5 + 0.5;
        // Depth is how far the level CAN swing; mix is how much of the
        // swung signal you are hearing. With the wet path muted there is no
        // tremolo however deep it is set, and the panel has to say so.
        let reach = depth * 0.80 * (0.12 + 0.88 * wet_mix);
        let env = mix(1.0 - reach, 1.0, swing);
        let carrier = sin(uv.x * 40.0 * TAU);
        field = carrier * env;
        field = field * step(mid, env);
    } else if (engine < VIBRATO + 0.5) {
        // Pitch, bending — drawn as a WAVEFORM whose wavelength stretches and
        // compresses, not as a field of bars.
        //
        // As bars it was a comb with uneven spacing, which is a flanger with
        // uneven spacing: the two engines came out looking like the same
        // picture, and they are not remotely the same effect. A single line
        // that visibly bunches and spreads says "the pitch is moving" in a
        // way a bar field cannot, because a bar field has no continuity for
        // the eye to follow along.
        let bend = depth * 1.3 * (0.15 + 0.85 * wet_mix)
            * sin((uv.x * 1.6 - phase) * TAU);
        let wave = sin((uv.x * 16.0 + bend * 6.0) * TAU);
        let y = 0.5 + wave * 0.30;
        let d = abs(uv.y - y);
        // The trace, plus a wider glow so it has body at a rack lane's
        // height.
        field = exp(-(d * d) / 0.0016) + exp(-d * 9.0) * 0.40;
        // Where the cycles bunch, the line is doing more — brighten it, so
        // the modulation peak is readable without counting wavelengths.
        let bunching = abs(cos((uv.x * 1.6 - phase) * TAU)) * depth;
        field = field * (0.75 + 0.55 * bunching);
    } else {
        // A horn going round, seen from above: the orbit it travels, the
        // horn on it, and the doppler it throws into the room.
        //
        // Measured in ASPECT-CORRECTED space. `distance(uv, c)` on a lane
        // that is three times wider than it is tall squashes the orbit into
        // a sliver and puts almost all of the falloff in x — which is why
        // this drew a single blob near the bottom and nothing else.
        let aspect = max(u.frame.x, 1.0) / max(u.frame.y, 1.0);
        let p = vec2<f32>((uv.x - 0.5) * aspect, uv.y - 0.5);

        let a = phase * TAU;
        let radius = 0.42 * (0.62 + 0.38 * depth);
        // Wider than tall, because a rotor seen from the front is an ellipse
        // and the lane is wide.
        let c = vec2<f32>(cos(a) * radius * 1.9, sin(a) * radius);
        let d = distance(p, c);

        // The orbit itself, faint: the path is context for the horn on it.
        let on_path = abs(length(vec2<f32>(p.x / 1.9, p.y)) - radius);
        // Bright enough to be the picture rather than a hint of one: the
        // orbit is what says "this thing goes round", and the horn alone is
        // a dot moving in the dark.
        let path = exp(-on_path * 13.0) * 0.55;

        // The horn. Coming toward you it is bright and tight; going away it
        // is dim and smeared — which IS the doppler, drawn rather than
        // described.
        let toward = sin(a) * 0.5 + 0.5;
        let tight = mix(45.0, 170.0, toward);
        let horn = exp(-d * d * tight) * (0.85 + 1.05 * toward);

        // The wake it drags behind itself around the orbit.
        var wake = 0.0;
        for (var i = 1u; i < 5u; i = i + 1u) {
            let lag = f32(i) * 0.16;
            let ca = a - lag;
            let cp = vec2<f32>(cos(ca) * radius * 1.9, sin(ca) * radius);
            let dd = distance(p, cp);
            wake = wake + exp(-dd * dd * 70.0) * (0.55 / f32(i));
        }

        // The room is the wet path: the orbit and the wake are what the
        // cabinet throws, and with the wet signal down you are left with the
        // horn itself.
        let room = 0.18 + 0.82 * wet_mix;
        field = path * room + horn + wake * room;
    }

    // Grain, so a flat region reads as material rather than as a fill.
    //
    // Sampled on SQUARE cells. Scaling uv by the panel's pixel size gave
    // cells as wide as the panel and as tall — 35 across and 12 down on a
    // rig lane — so the grain came out as vertical streaks, and on a quiet
    // engine those streaks were the brightest thing on the panel. Square
    // cells read as material; rectangular ones read as a pattern the effect
    // is not making.
    let cell = max(u.frame.x, u.frame.y) * 0.05;
    let grain = (noise(uv * cell + t * 0.3) - 0.5) * 0.10;
    var lit = clamp(abs(field) + grain, 0.0, 1.5);

    // A bypassed engine keeps its shape and loses its light — "off" and
    // "not configured" must not look the same.
    lit = lit * mix(0.16, 1.0, engaged);

    // Colour: the group's hue, lifted toward white where the field is
    // strongest, so the bright parts read as energy rather than as a
    // different colour arriving.
    // The brightest parts are MORE of the group's colour, not less. Mixing
    // toward white meant a chorus knot or a dense comb turned into a white
    // highlight exactly where the picture had the most to say — and the
    // colour is what tells you whether you are reading MOD or MOTION.
    let base = u.color.rgb;
    let heat = clamp(lit - 0.45, 0.0, 1.0);
    let rgb = tonemap(hotter(base, heat * 1.15) * clamp(lit, 0.0, 1.4));
    let alpha = clamp(lit * 0.82, 0.0, 0.92);

    // ── The lane itself ─────────────────────────────────────────────────
    //
    // The panel is a tinted FIELD, not a black box with coloured marks on
    // it. Modulation is cyan-led and motion pink-led, and that grouping is
    // how you know which of the two rows you are reading without going to
    // the label — but it only worked where the engine happened to be
    // drawing. On a quiet engine, or in the wide empty margins a narrow
    // shape leaves, the panel was black and said nothing at all.
    //
    // Deep and desaturated: this is UNDER the field, and a ground bright
    // enough to compete with it would cost the picture its contrast. It also
    // keeps its colour into the corners rather than fading out, which is
    // exactly where the panel stops identifying itself.
    let ground_base = mix(vec3<f32>(0.012, 0.018, 0.024), base, 0.13);
    // A little brighter through the middle, where the engines live.
    let across = mix(1.20, 0.72, clamp(mid, 0.0, 1.0));
    let along = mix(1.06, 0.88, abs(uv.x - 0.5) * 2.0);
    // Bypassed keeps a ground, dimmed: an engine that is off should look
    // switched off, not unloaded.
    let ground_rgb = ground_base * across * along * mix(0.45, 1.0, engaged);
    let edge = 1.0 - smoothstep(0.80, 1.0, mid) * 0.40;
    // An inner shadow along the top, and a hairline of light along the
    // bottom. Costs almost nothing and is the difference between a panel
    // that sits IN the rack and a rectangle of colour laid on top of it.
    let inset = 1.0 - exp(-uv.y * 34.0) * 0.55;
    let sill = exp(-(1.0 - uv.y) * 46.0) * 0.14;
    let ground_a = 0.94 * edge;
    let ground_lit = ground_rgb * inset + base * sill;

    // Field over ground, composited rather than summed. The ground is not
    // light the panel is emitting, it is the panel.
    let out_a = alpha + ground_a * (1.0 - alpha);
    let out_rgb = rgb * alpha + ground_lit * ground_a * (1.0 - alpha);
    return vec4<f32>(out_rgb, out_a);
}

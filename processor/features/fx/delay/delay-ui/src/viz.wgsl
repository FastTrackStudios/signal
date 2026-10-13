// The delay's taps, as light.
//
// The vector pass draws the same taps as geometry and is a perfectly good
// picture; what a shader adds is the part that is about energy rather than
// position — the bloom as the playhead crosses a repeat, the haze the tail
// leaves behind it, the way a loud tap spills past its own edge. Those are
// per-pixel effects, and drawing them as geometry means one shape per glow
// and a scene that grows with the tail.
//
// Layout of `u` mirrors `DelayUniforms` in viz.rs. `taps` is fixed-length
// because a uniform buffer is; `frame.w` carries how many are real.
//
// The beat grid is NOT here. A 1 px ruled line is the one thing the vector
// pass does better, and the grid is what the taps are read against.

const MAX_TAPS: u32 = 32u;
const TAU: f32 = 6.28318530718;

// The families, in `family_index`'s order. The two must agree.
const DIGITAL:  f32 = 0.0;
const TAPE:     f32 = 1.0;
const ANALOG:   f32 = 2.0;
const PITCH:    f32 = 3.0;
const RHYTHMIC: f32 = 4.0;
const SPECIAL:  f32 = 5.0;

struct Delay {
    // width, height, seconds, tap_count
    frame: vec4<f32>,
    // window_s, mix, on, beat_s
    params: vec4<f32>,
    // the lane's colour; w unused
    color: vec4<f32>,
    // family, unused, unused, unused
    style: vec4<f32>,
    // at_s, level, pan, unused
    taps: array<vec4<f32>, MAX_TAPS>,
};

@group(0) @binding(0) var<uniform> u: Delay;

fn is_family(f: f32) -> bool {
    return abs(u.style.x - f) < 0.5;
}

fn hash1(p: f32) -> f32 {
    return fract(sin(p * 127.1) * 43758.5453);
}

// How much the machine smears a repeat by the time it is `age` repeats old.
//
// This is the single number that separates the families most: a digital
// delay's tenth repeat is the first one again, a tape's has been through the
// heads ten times, and a bucket-brigade's has been resampled ten times by a
// clock that was never clean. Drawn as the bar getting wider and losing its
// cap, because that is what the ear hears as the repeat going soft.
fn smear(age: f32) -> f32 {
    if (is_family(TAPE)) { return age * 1.6; }
    if (is_family(ANALOG)) { return age * 3.2; }
    if (is_family(SPECIAL)) { return age * 1.4; }
    return 0.0;
}

// Wow and flutter: the horizontal wobble a mechanical transport puts on a
// repeat. Zero for everything that is not a transport — a digital delay that
// wobbled would be lying about the one thing it is for.
fn wobble(age: f32, t: f32) -> f32 {
    // In PIXELS of the panel, so the wobble is as visible on a rack lane as
    // it is on a plugin window. A constant here is invisible on one and
    // absurd on the other.
    let unit = max(u.frame.x * 0.012, 4.0);
    if (is_family(TAPE)) {
        // Wow is slow and deep, flutter fast and shallow; both grow with how
        // many passes the repeat has had. This is the single cue that a
        // transport is involved, so it has to be plainly visible — a repeat
        // that has been round the reels five times is not where the
        // arithmetic says it should be, and that is the whole point.
        return (sin(t * 0.7 + age * 4.0) * 1.0 + sin(t * 6.1 + age * 9.0) * 0.30)
            * age * unit;
    }
    if (is_family(ANALOG)) {
        return sin(t * 1.1 + age * 6.0) * 0.55 * age * unit;
    }
    return 0.0;
}

// The playhead's position across the window, 0..=1, or −1 when the lane is
// bypassed. A bypassed delay keeps its shape and loses its motion: "off" and
// "not configured" must not look the same.
fn playhead() -> f32 {
    let window = max(u.params.x, 1e-3);
    let beat = max(u.params.w, 1e-3);
    if (u.params.z < 0.5) { return -1.0; }
    let cycle = max(beat * ceil(window / beat), beat);
    return fract(u.frame.z / cycle);
}

// How recently the playhead passed `at` (seconds), 0..=1, dying within a beat
// so two taps a beat apart never glow at once and the eye follows a single
// moving highlight.
fn recency(at: f32, head: f32) -> f32 {
    if (head < 0.0) { return 0.0; }
    let window = max(u.params.x, 1e-3);
    let beat = max(u.params.w, 1e-3);
    let cycle = max(beat * ceil(window / beat), beat);
    let pos = at / window;
    let since = fract(head - pos + 1.0);
    let over = beat / cycle;
    if (since >= over) { return 0.0; }
    let k = 1.0 - since / over;
    return k * k * k;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let px = uv * vec2<f32>(u.frame.x, u.frame.y);
    let t = u.frame.z;
    let lit = u.params.z > 0.5;

    let window = max(u.params.x, 1e-3);
    let mid = u.frame.y * 0.5;
    let head = playhead();

    // Bypassed keeps the picture and drops the colour, so the shape still
    // reads as a configured delay that happens to be off.
    var tint = u.color.rgb;
    if (!lit) { tint = vec3<f32>(0.32, 0.32, 0.36); }
    let hot = hotter(tint, 0.55);

    var rgb = vec3<f32>(0.0);
    var alpha = 0.0;

    // ── The lane itself ─────────────────────────────────────────────────
    //
    // The panel is a DEEP BLUE field, not a black box with blue marks on it.
    // A near-black ground made every time-effect lane look like every other
    // one, and left the rig's two rows of them — delay and reverb — telling
    // you apart only by the colour of the few lit pixels. The ground is
    // where a panel says what it is, before anything is drawn on it.
    //
    // Deep and desaturated: this sits UNDER the taps, and a ground bright
    // enough to compete with them would cost the picture its contrast. It is
    // painted with its own alpha rather than added, so what shows through is
    // the lane's colour and not the rack's black.
    let base = mix(vec3<f32>(0.010, 0.013, 0.030), tint, 0.10);
    // Brighter at the head of the lane, where the dry hit is and the first
    // repeats land; the far end is where the tail has given up.
    // The far end keeps its colour rather than going black: a lane that
    // fades to the rack's own ground stops saying which effect it is
    // exactly where the tail is hardest to read.
    let along = mix(1.25, 0.72, uv.x);
    // And brighter along the channel rule, so the lane has a spine.
    let across = mix(1.15, 0.72, clamp(abs(uv.y - 0.5) * 2.0, 0.0, 1.0));
    let ground_rgb = base * along * across;
    // A vignette, so the lane reads as a panel rather than as a rectangle
    // of colour butted against its neighbours.
    let edge_y = 1.0 - smoothstep(0.78, 1.0, abs(uv.y - 0.5) * 2.0) * 0.45;
    let ground_a = 0.95 * edge_y;

    // ── What the machine puts in the lane itself ────────────────────────
    //
    // Two families are not characterised by what they do to a repeat but by
    // where the repeats are allowed to be, so their mark is on the lane
    // rather than on the bars.
    if (is_family(RHYTHMIC) && lit) {
        // The grid IS the effect, so it is drawn as one: sixteenth SLOTS,
        // every one of them, lit whether or not a repeat landed in it. A
        // rhythmic delay places its repeats on a pattern rather than at
        // multiples of one time, and a pattern is only readable against the
        // slots it could have used — an uneven row of sticks with nothing
        // behind it is just an uneven row of sticks.
        let beat = max(u.params.w, 1e-3);
        let slots = max(floor(window / (beat * 0.25) + 0.5), 1.0);
        let slot = uv.x * slots;
        let which = floor(slot);
        let inside = fract(slot);

        // Every slot gets a step pad, the downbeats brighter, so the bar
        // line is countable without a caption. Bright enough to be furniture
        // you read the pattern ON, which is the whole job — at a tenth of
        // this it was a smudge and the family was indistinguishable from
        // Digital.
        let is_beat = abs(which - floor(which / 4.0) * 4.0) < 0.5;
        let pad_h = select(0.055, 0.085, is_beat);
        let pad = (1.0 - smoothstep(pad_h * 0.55, pad_h, abs(uv.y - 0.5)))
            * (1.0 - smoothstep(0.62, 0.90, abs(inside - 0.5) * 2.0));
        let lit_pad = pad * select(0.30, 0.55, is_beat);
        rgb += mix(tint, hot, 0.25) * lit_pad;
        alpha += lit_pad * 0.8;

        // A full-height rail on the downbeats: the bar lines the pattern is
        // counted against.
        if (is_beat) {
            let rail = smoothstep(0.86, 1.0, abs(inside - 0.5) * 2.0) * 0.16;
            rgb += tint * rail;
            alpha += rail * 0.7;
        }
    }
    if (is_family(DIGITAL) && lit) {
        // A quantisation lattice: fine, exact, evenly spaced, and utterly
        // regular. A digital delay's character is that it does the
        // arithmetic and nothing else, which is a hard thing to DRAW —
        // "absence of artefact" has no shape. The grid is the positive
        // version of it: the machine's own ruler, visible behind the
        // repeats that land exactly on it.
        let ticks = 64.0;
        let cell = fract(uv.x * ticks);
        let tick = smoothstep(0.90, 1.0, abs(cell - 0.5) * 2.0);
        let near_rule = exp(-abs(uv.y - 0.5) * 7.0);
        let lattice = tick * near_rule * 0.09;
        rgb += tint * lattice;
        alpha += lattice * 0.6;
    }
    if (is_family(TAPE) && lit) {
        // The transport itself: the tape path running the length of the
        // lane, wowing as it goes. The repeats sit ON it, which is why they
        // are not where the arithmetic says — the picture shows the thing
        // that moved them rather than only the result.
        let wow = sin(uv.x * 4.0 + t * 0.7) * 0.10 + sin(uv.x * 9.0 - t * 2.3) * 0.025;
        let path = exp(-abs((uv.y - 0.5) - wow) * 26.0);
        let ribbon = path * (0.10 + 0.05 * sin(uv.x * 30.0 - t * 4.0));
        rgb += mix(tint, vec3<f32>(1.0, 0.72, 0.42), 0.55) * ribbon;
        alpha += ribbon * 0.55;
    }
    if (is_family(ANALOG) && lit) {
        // A bucket brigade's noise floor, rising along the lane. The chip
        // hisses and the clock leaks, and by the end of the tail there is
        // more of that than there is repeat — which is the reason nobody
        // runs one at high feedback and is invisible in a clean drawing.
        let grain = hash1(floor(px.x * 0.9) * 2.3 + floor(px.y * 0.9) * 7.1 + floor(t * 12.0));
        let floor_h = exp(-abs(uv.y - 0.5) * 4.5);
        let hiss = grain * floor_h * (0.05 + 0.30 * uv.x) * 0.22;
        rgb += mix(tint, vec3<f32>(0.42, 0.36, 0.50), 0.6) * hiss;
        alpha += hiss * 0.7;
    }
    if (is_family(SPECIAL) && lit) {
        // A repeat that is no longer one: reversed, filtered, dissolved.
        // The lane itself is unstable — a drifting veil that says the
        // repeats are being taken apart rather than merely fading.
        // Grain that thickens along the lane: the repeats are coming apart,
        // and by the end of the tail there is more cloud than repeat. A flat
        // wash would say "this panel has a haze on it" rather than "this
        // machine is dissolving what you put into it".
        let drift = vec2<f32>(uv.x * 26.0 - t * 0.5, (uv.y - 0.5) * 14.0 + t * 0.3);
        let cell = floor(drift);
        let grit = hash1(cell.x * 3.7 + cell.y * 11.3);
        let puff = exp(-length(fract(drift) - 0.5) * 4.0) * step(0.55, grit);
        let dissolve = puff * (0.12 + 0.85 * uv.x) * 0.30;
        rgb += mix(tint, hot, 0.55) * dissolve;
        alpha += dissolve * 0.7;
    }

    // ── The channel rule ────────────────────────────────────────────────
    //
    // The lane is split: UP IS LEFT, DOWN IS RIGHT. That is the read every
    // good delay display uses (FabFilter's Timeless puts the channels on the
    // vertical axis for the same reason), and it is what makes a ping-pong
    // legible — the repeats alternate across the rule instead of nudging a
    // few pixels off a shared centre line, which is what a pan-as-offset
    // draws and which nobody can see.
    let rule = exp(-abs(px.y - mid) / max(u.frame.y * 0.004, 0.7)) * 0.20;
    rgb += tint * rule;
    alpha += rule * 0.55;

    // ── The dry hit ─────────────────────────────────────────────────────
    //
    // At zero, full height, across both channels: everything to the right of
    // it is a repeat OF this, which is the one relationship the picture has
    // to establish.
    let dry_w = max(u.frame.x * 0.0018, 1.2);
    let dry = exp(-(px.x * px.x) / (dry_w * dry_w * 4.0));
    rgb += hot * dry * 0.9;
    alpha += dry * 0.8;

    // ── The taps ────────────────────────────────────────────────────────
    let count = u32(max(u.frame.w, 0.0));
    for (var i = 0u; i < MAX_TAPS; i = i + 1u) {
        if (i >= count) { break; }
        let tap = u.taps[i];
        let at = tap.x;
        let level = clamp(tap.y, 0.0, 1.0);
        let pan = clamp(tap.z, -1.0, 1.0);
        if (at > window) { continue; }

        let hit = recency(at, head);
        // How far down the tail this repeat is, 0..1 — measured in TIME, not
        // in array position.
        //
        // The index is not the age. A stereo delay reports its taps as two
        // runs concatenated: every left repeat, then every right repeat. By
        // index, the right channel's FIRST repeat sits halfway through the
        // array and came out drawn as a half-aged one — on a tape delay that
        // meant the right channel was orange and smeared from its very first
        // repeat while the left was still clean blue, which reads as a fault
        // in the picture because it is one.
        //
        // Time is side-independent and is what the machine has actually had
        // to work on: two repeats that land together have had the same done
        // to them, whichever channel they are in.
        let age = clamp(at / window, 0.0, 1.0);
        let sm = smear(age);
        let x = at / window * u.frame.x + wobble(age, t);

        // Equal-power pan, so a tap's two bars carry its level between them
        // the way the pan law does. A centred tap is half height on both
        // sides rather than full height on neither.
        let ang = (pan * 0.5 + 0.5) * 1.5707963;
        let gain_l = cos(ang);
        let gain_r = sin(ang);
        // Which side of the rule this pixel is on, and how far into it.
        let up = px.y < mid;
        let gain = select(gain_r, gain_l, up);
        // Room for the bar, measured from the rule out to the lane's edge.
        let half = u.frame.y * 0.5;
        let from_rule = abs(px.y - mid);
        let bar = level * gain * half * 0.92 * (1.0 + 0.10 * hit);

        let dx = px.x - x;

        // ── What this machine does to a repeat's colour ─────────────────
        //
        // Pitch delays move the repeat, so the repeat moves through the
        // spectrum: each one is further up or down from the one before, and
        // a shimmer climbing an octave a repeat should LOOK like it climbs.
        // Everything else keeps the lane's hue and only loses brightness.
        var voice = tint;
        if (is_family(PITCH)) {
            // Each repeat is a different NOTE, so each repeat is a different
            // colour — the full sweep, not a tint. A shimmer climbing an
            // octave a repeat should look like it climbs.
            let up = vec3<f32>(0.35, 1.00, 0.75);
            let down = vec3<f32>(1.00, 0.45, 0.30);
            // Alternating sides climb and fall independently, which is what
            // a dual-tap pitch delay actually does.
            let rising = pan < 0.0;
            voice = mix(tint, select(down, up, rising), min(age * 1.5, 1.0));
        } else if (is_family(TAPE)) {
            // Oxide: the repeats go warm as they go soft. Pushed far enough
            // to read at a glance — the last repeat of a tape delay is a
            // different colour from the first, and that IS what it sounds
            // like.
            voice = mix(tint, vec3<f32>(1.0, 0.58, 0.22), min(age * 1.25, 0.95));
        } else if (is_family(ANALOG)) {
            // A bucket brigade loses the top first and ends up muddy.
            voice = mix(tint, vec3<f32>(0.42, 0.36, 0.50), min(age * 1.35, 0.9));
        }

        // The bar: bright along its own column, growing from the rule
        // outward and stopping at its own height.
        //
        // Width is in PIXELS and must not be a constant: a falloff tuned on a
        // 600 px panel is a hairline on a 2560 px one, which is how these
        // ended up all but invisible on the rig's own screen. Scaled off the
        // panel, with a floor so a narrow lane still draws something.
        let stem_w = max(u.frame.x * 0.0022, 1.4) * (1.0 + 0.5 * hit) * (1.0 + sm * 3.5);
        // A smeared repeat loses its edge as well as its width: the bar
        // stops ending anywhere in particular.
        let edge_lo = mix(0.90, 0.35, clamp(sm, 0.0, 1.0));
        let within = 1.0 - smoothstep(bar * edge_lo, bar * (1.06 + sm), from_rule);
        let stem = exp(-(dx * dx) / (stem_w * stem_w)) * within;
        // Tape and analog darken as they smear; a digital repeat does not.
        let dull = 1.0 / (1.0 + sm * 1.6);
        rgb += voice * stem * (0.55 + 0.85 * level + 0.7 * hit) * dull;
        alpha += stem * (0.55 + 0.45 * level + 0.35 * hit);

        // The cap: a bright point at the bar's far end, which is where its
        // level is actually read.
        let r = max(u.frame.x * 0.0035, 2.2) + 2.5 * level + 3.0 * hit;
        let cap_y = mid + select(bar, -bar, up);
        let d = length(vec2<f32>(dx, px.y - cap_y));
        // The cap goes as the repeat smears — a soft repeat has no edge to
        // read a level off, which is exactly the point.
        let cap = exp(-(d * d) / (r * r * 0.8)) * step(0.02, bar) / (1.0 + sm * 2.4);
        // The cap carries the repeat's OWN colour, lifted toward white
        // rather than replaced by it. Always-hot caps meant a pitch delay's
        // repeats — whose whole character is that each one is a different
        // note — differed from a digital delay's only along a two-pixel
        // stem, which is to say not at all.
        let cap_rgb = hotter(voice, 0.45);
        rgb += cap_rgb * cap * (0.5 + 0.5 * level + 0.5 * hit);
        alpha += cap * (0.5 + 0.4 * level + 0.4 * hit);

        // A pitch delay's rung: a short horizontal tick beside each repeat,
        // stepped further from the centre with every one. The colour says a
        // note changed; the ladder says BY HOW MUCH, and in which direction
        // — an interval is a distance and a distance wants to be drawn as
        // one.
        if (is_family(PITCH)) {
            let rising = pan < 0.0;
            let step_n = age * 5.0;
            let rung_y = mid + select(1.0, -1.0, rising) * u.frame.y * 0.055 * step_n;
            let rung_w = max(u.frame.x * 0.010, 5.0);
            let rx = 1.0 - smoothstep(rung_w * 0.55, rung_w, abs(dx));
            let ry = exp(-abs(px.y - rung_y) / max(u.frame.y * 0.008, 1.0));
            let rung = rx * ry * 0.55;
            rgb += hotter(voice, 0.3) * rung;
            alpha += rung * 0.65;
        }

        // A pitch delay's repeats get a standing halo in their own colour,
        // not only a lit one. The note each repeat lands on is the whole
        // character of the family, and a colour that only exists on a stem
        // is a colour nobody sees.
        if (is_family(PITCH)) {
            let halo = exp(-d / (r * 5.0)) * level * 0.45;
            rgb += voice * halo;
            alpha += halo * 0.45;
        }

        // A reversed repeat swells INTO its hit instead of starting at it.
        // That is the whole character of the family, and it is the one thing
        // a row of decaying sticks cannot say.
        if (is_family(SPECIAL)) {
            let ramp_len = max(u.frame.x * 0.045, 6.0);
            let before = clamp((x - px.x) / ramp_len, 0.0, 1.0);
            let swell = (1.0 - before) * step(px.x, x) * within * level * 0.5;
            rgb += voice * swell;
            alpha += swell * 0.5;
        }

        // The bloom, only while lit by the sweep. This is the part that is
        // worth a GPU: one soft falloff per tap, per pixel, for free.
        if (hit > 0.01) {
            let br = 4.0 + 18.0 * hit * (0.4 + level);
            let bloom = exp(-d / br) * hit;
            rgb += voice * bloom * 0.55;
            alpha += bloom * 0.30;
        }
    }

    // ── The envelope ────────────────────────────────────────────────────
    //
    // The shape the tail decays along, as a haze rather than an outline: it
    // is context for the taps, and an edge would compete with them.
    if (count > 1u) {
        // Nearest-tap level at this x, so the haze follows the actual taps
        // rather than an idealised exponential the delay may not be doing.
        // Per SIDE, so the haze follows each channel's own tail. A shared
        // envelope would draw a ping-pong as one symmetrical shape, which is
        // the opposite of what it sounds like.
        var env_l = 0.0;
        var env_r = 0.0;
        for (var i = 0u; i < MAX_TAPS; i = i + 1u) {
            if (i >= count) { break; }
            let tap = u.taps[i];
            let tx = tap.x / window;
            let w = exp(-abs(uv.x - tx) * 12.0);
            let a = (clamp(tap.z, -1.0, 1.0) * 0.5 + 0.5) * 1.5707963;
            env_l = max(env_l, tap.y * cos(a) * w);
            env_r = max(env_r, tap.y * sin(a) * w);
        }
        let env = select(env_r, env_l, uv.y < 0.5);
        let span = env * 0.46;
        let inside = 1.0 - smoothstep(span * 0.75, span * 1.15, abs(uv.y - 0.5));
        let haze = inside * env * 0.40 * (1.0 - uv.x * 0.55);
        rgb += mix(tint, hot, 0.5) * haze;
        alpha += haze * 0.55;
    }

    // ── The sweep ───────────────────────────────────────────────────────
    //
    // A thin bright edge with a trail behind it. Not decoration: the taps are
    // static geometry saying *where* the repeats land, and the sweep is what
    // turns that into a rhythm readable at a glance.
    if (head >= 0.0) {
        let hx = head * u.frame.x;
        let behind = max(hx - px.x, 0.0);
        let trail = exp(-behind / (u.frame.x * 0.08)) * 0.16;
        let edge = exp(-abs(px.x - hx) / max(u.frame.x * 0.0012, 1.0)) * 0.55;
        rgb += tint * trail + hot * edge;
        alpha += trail * 0.7 + edge * 0.6;
        // A faint shimmer riding the edge, so the sweep has a leading face.
        let shimmer = edge * 0.25 * (0.5 + 0.5 * sin(px.y * 0.35 + t * 6.0));
        rgb += hot * shimmer;
        alpha += shimmer * 0.4;
    }

    // Tone-map rather than clamp: two overlapping blooms otherwise clip to
    // white and lose the colour in both, exactly where it matters most.
    // Driven before the tone map. Reinhard on raw contributions crushes
    // everything below 1.0, and 1.0 is brighter than anything this panel
    // produces — so without the drive the whole picture sits in the bottom
    // quarter of the curve and reads as switched off.
    rgb = tonemap(rgb * 2.2);
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

// The EQ's light: the spectrum as a field, and a bloom under every band node.
//
// Painted UNDER the vector graph, not instead of it. The curves, the grid and
// the node rings stay crisp geometry — they are read precisely, and a shader
// is the wrong tool for a 1 px line. What a shader is the right tool for is
// everything that should look like energy rather than like a drawing: the
// analyser's body, the haze where a band is lifting, the halo that says which
// node has focus.
//
// Layout of `u` mirrors `Glow` in eq_glow.rs. The arrays are fixed-length
// because a uniform buffer is: `bins` and `nodes` carry their own counts.

const MAX_BINS: u32 = 128u;
const MAX_NODES: u32 = 24u;
const TAU: f32 = 6.28318530718;

struct Glow {
    // width, height, seconds, bin_count
    frame: vec4<f32>,
    // node_count, db_range, spectrum_floor_db, unused
    cfg: vec4<f32>,
    // node_glow, spectrum_glow, hue_spread, bloom — the look, mirroring
    // `GlowStyle` in eq_glow.rs. Everything else here is measurement; this
    // row is the only taste.
    style: vec4<f32>,
    // Each bin's magnitude in dB, packed four to a row.
    bins: array<vec4<f32>, 32>,
    // x, y, radius, strength — in pixels, already laid out by the graph.
    nodes: array<vec4<f32>, MAX_NODES>,
    // Each node's colour; w is unused.
    node_color: array<vec4<f32>, MAX_NODES>,
};

@group(0) @binding(0) var<uniform> u: Glow;

// `VsOut` and `vs_main` come from the shared prelude — every panel draws the
// same full-screen triangle, and a second copy here is a redefinition the
// device rejects.

// One bin, by index, out of the packed rows.
fn bin_at(i: u32) -> f32 {
    let row = u.bins[i / 4u];
    let lane = i % 4u;
    if (lane == 0u) { return row.x; }
    if (lane == 1u) { return row.y; }
    if (lane == 2u) { return row.z; }
    return row.w;
}

// The analyser's height at this x, 0..=1, smoothed across neighbours so the
// field reads as a body rather than as a bar chart.
fn spectrum_height(x: f32) -> f32 {
    let count = u32(max(u.frame.w, 1.0));
    if (count < 2u) { return 0.0; }
    let floor_db = u.cfg.z;
    let pos = clamp(x, 0.0, 1.0) * f32(count - 1u);
    let i = u32(floor(pos));
    let f = fract(pos);
    let j = min(i + 1u, count - 1u);
    let a = bin_at(i);
    let b = bin_at(j);
    let db = mix(a, b, f);
    return clamp((db - floor_db) / (0.0 - floor_db), 0.0, 1.0);
}

// The analyser's hue at this point on the frequency axis.
//
// The graph already colours a band by where it sits — a low shelf is not the
// same colour as an air band — and the analyser using the same sweep means the
// light under a node and the node itself agree. It also does the work a single
// colour cannot: with one hue the only thing carrying frequency is horizontal
// position, and on a log axis that is exactly where the eye is worst at it.
//
// `hue_spread` at 0 collapses the sweep to one hue, for a panel that wants the
// analyser to read as one object.
fn spectrum_hue(x: f32, energy: f32) -> vec3<f32> {
    let low = vec3<f32>(1.00, 0.36, 0.42);
    let mid = vec3<f32>(0.45, 0.85, 0.55);
    let high = vec3<f32>(0.40, 0.72, 1.00);
    let air = vec3<f32>(0.78, 0.60, 1.00);

    let k = clamp(x, 0.0, 1.0) * 3.0;
    var swept = mix(low, mid, clamp(k, 0.0, 1.0));
    swept = mix(swept, high, clamp(k - 1.0, 0.0, 1.0));
    swept = mix(swept, air, clamp(k - 2.0, 0.0, 1.0));

    // The neutral the sweep collapses towards.
    let flat = vec3<f32>(0.52, 0.80, 1.0);
    let hue = mix(flat, swept, clamp(u.style.z, 0.0, 1.0));
    // Loud runs HOTTER, not whiter: a peak should look like more of its own
    // frequency band, and a white peak has thrown away the one thing the
    // hue sweep was drawn to say.
    return hotter(hue, 0.55 * energy * energy);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let px = uv * vec2<f32>(u.frame.x, u.frame.y);
    let t = u.frame.z;

    let node_glow = max(u.style.x, 0.0);
    let spectrum_glow = max(u.style.y, 0.0);
    let bloom = max(u.style.w, 0.0);

    var rgb = vec3<f32>(0.0);
    var alpha = 0.0;

    // ── The analyser ────────────────────────────────────────────────────
    //
    // A filled body with a lit rim, plus a soft bloom above it. The bloom is
    // what makes a loud band look loud rather than just tall: energy spilling
    // past its own edge is how light behaves, and the eye reads it before it
    // reads a height.
    //
    // This is the ONLY analyser on the panel. The vector pass is told to skip
    // its own (`GraphPaint::spectrum`), because the same data drawn twice with
    // two different smoothings reads as the analyser being wrong rather than
    // as a second layer.
    let h = spectrum_height(uv.x);
    let top = 1.0 - h;
    let below = smoothstep(0.0, 0.012, uv.y - top);
    let body = below * (0.10 + 0.45 * h);

    // The rim, tight to the edge.
    let rim = exp(-abs(uv.y - top) * 190.0) * (0.35 + 0.65 * h);
    // The spill above it, wider where there is more energy.
    let spill = exp(-max(top - uv.y, 0.0) * (26.0 - 12.0 * h)) * 0.30 * h * bloom;
    // A vertical shimmer riding the rim, so a held note is alive rather than a
    // frozen outline. Tied to the energy, so silence is genuinely still.
    let shimmer = exp(-abs(uv.y - top) * 70.0)
        * 0.18 * h * (0.5 + 0.5 * sin(uv.x * 46.0 + t * 2.1));

    let spectrum_rgb = spectrum_hue(uv.x, h);

    rgb += spectrum_rgb * ((body + rim * 1.5 + spill + shimmer) * spectrum_glow);
    alpha += (body * 0.55 + rim * 0.85 + spill * 0.7 + shimmer * 0.5) * spectrum_glow;

    // ── The bands ───────────────────────────────────────────────────────
    //
    // A halo under each node, in the node's own colour, sized by its radius
    // and breathing very slightly so a static graph is never quite still.
    let count = u32(max(u.cfg.x, 0.0));
    for (var i = 0u; i < MAX_NODES; i = i + 1u) {
        if (i >= count) { break; }
        let n = u.nodes[i];
        let strength = n.w;
        if (strength <= 0.001) { continue; }

        let d = distance(px, n.xy);
        let r = max(n.z, 1.0);
        // Breath is per-node and out of phase, so a row of bands shimmers
        // rather than pulsing in unison like a warning light.
        let breath = 1.0 + 0.06 * sin(t * TAU * 0.35 + f32(i) * 1.7);
        let core = exp(-(d * d) / (r * r * 0.55 * breath));
        let halo = exp(-d / (r * 3.2 * breath)) * 0.45;
        // A tight bright centre on top of the two soft falloffs, so the node
        // reads as a source of the light rather than as a patch of it.
        let spark = exp(-(d * d) / (r * r * 0.05)) * 0.9;
        let lit = (core + halo + spark * bloom) * strength * node_glow;

        // The band's own colour, undiluted at the centre and kept saturated in
        // the falloff. Each node glowing its OWN hue is the whole point: it is
        // what ties a halo to the ring drawn over it and to its place on the
        // frequency sweep.
        // The band's own colour, intensified at the centre rather than
        // bleached. Two bands whose halos overlap should pile up into a
        // deeper mix of their colours, not a white patch.
        let tint = hotter(u.node_color[i].rgb, 0.45 * spark);
        rgb += tint * lit;
        alpha += lit * 0.60;
    }

    // Tone-map rather than clamp: without this two overlapping halos clip to
    // white and the colour information in both is lost exactly where they
    // matter most.
    rgb = tonemap(rgb);
    alpha = clamp(alpha, 0.0, 0.88);

    // Premultiplied, because that is what the compositor expects of the
    // texture it is handed.
    return vec4<f32>(rgb * alpha, alpha);
}

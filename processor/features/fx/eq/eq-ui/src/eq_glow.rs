//! The EQ's light: the analyser as a field, and a bloom under every band.
//!
//! # Why a shader and not more vectors
//!
//! The graph's geometry is read precisely — a curve says what the filter does,
//! a grid line says where 1 kHz is, a node ring says where to grab. Vectors are
//! right for all of it, and a shader would only make a 1 px line worse.
//!
//! What vectors are wrong for is everything that should look like *energy*:
//! the analyser's body, the spill where a band is loud, the halo that says
//! which node has focus. Those want the thing light actually does — falling
//! off smoothly, adding where it overlaps, blowing out toward white — and
//! approximating that with stacked translucent paths costs both fidelity and
//! fill rate.
//!
//! So this paints UNDER the vector graph. The shader is the light; the scene
//! on top of it is the drawing.
//!
//! # Fixed-length arrays
//!
//! A uniform buffer has a fixed size, so the bins and the nodes are arrays
//! with their own counts rather than slices. 128 bins is more than the graph
//! draws at any sane width, and 24 nodes is the band ceiling
//! ([`MAX_BANDS`](crate::eq_graph_model::MAX_BANDS)).

use fts_audio_ui::shader::ShaderSurface;

/// How many analyser bins the shader carries.
pub const MAX_BINS: usize = 128;
/// How many band halos the shader carries.
pub const MAX_NODES: usize = 24;

/// The uniform block, laid out to match `Glow` in `eq_glow.wgsl`.
///
/// Every row is a `vec4`, so there is no padding for the two languages to
/// disagree about.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Glow {
    /// `[width, height, seconds, bin_count]`.
    pub frame: [f32; 4],
    /// `[node_count, db_range, spectrum_floor_db, _]`.
    pub cfg: [f32; 4],
    /// The look, and the only thing here that is taste rather than data:
    /// `[node_glow, spectrum_glow, hue_spread, bloom]`. See [`GlowStyle`].
    pub style: [f32; 4],
    /// Bin magnitudes in dB, packed four per row.
    pub bins: [[f32; 4]; MAX_BINS / 4],
    /// `[x, y, radius, strength]` in pixels.
    pub nodes: [[f32; 4]; MAX_NODES],
    /// Each node's colour, `w` unused.
    pub node_color: [[f32; 4]; MAX_NODES],
}

/// The dials on the look of the light.
///
/// Separate from the data rows because it is the half of the shader that is a
/// choice rather than a measurement: the same spectrum and the same bands can
/// read as a clinical analyser or as a lit instrument, and which one a panel
/// wants depends on where it is — a rig's rack panel and the plugin's own
/// full-size window are not after the same picture.
#[derive(Clone, Copy, Debug)]
pub struct GlowStyle {
    /// Brightness of each band's halo, 0..~2.
    pub node_glow: f32,
    /// Brightness of the analyser's body and rim, 0..~2.
    pub spectrum_glow: f32,
    /// How far the analyser's colour travels across the frequency axis, 0..1.
    /// At 0 it is one hue; at 1 it runs the full sweep low → high.
    pub hue_spread: f32,
    /// How far energy spills past its own edge, 0..~2. This is what makes a
    /// loud band look loud rather than merely tall.
    pub bloom: f32,
}

impl Default for GlowStyle {
    fn default() -> Self {
        Self {
            node_glow: 1.0,
            spectrum_glow: 1.0,
            hue_spread: 1.0,
            bloom: 1.0,
        }
    }
}

impl GlowStyle {
    /// The row the shader reads.
    #[must_use]
    pub fn pack(self) -> [f32; 4] {
        [
            self.node_glow,
            self.spectrum_glow,
            self.hue_spread,
            self.bloom,
        ]
    }
}

impl Default for Glow {
    fn default() -> Self {
        Self {
            frame: [0.0; 4],
            cfg: [0.0; 4],
            style: GlowStyle::default().pack(),
            bins: [[0.0; 4]; MAX_BINS / 4],
            nodes: [[0.0; 4]; MAX_NODES],
            node_color: [[0.0; 4]; MAX_NODES],
        }
    }
}

impl Glow {
    /// Write the analyser in, resampling to [`MAX_BINS`].
    ///
    /// The graph's analyser has whatever resolution the DSP gave it; the
    /// shader's array is fixed. Resampling here rather than in WGSL keeps the
    /// shader's indexing trivial and means a change of analyser size costs
    /// nothing on the GPU.
    pub fn set_spectrum(&mut self, spectrum: &[f32], floor_db: f32) {
        self.cfg[2] = floor_db;
        if spectrum.len() < 2 {
            self.frame[3] = 0.0;
            return;
        }
        let out = MAX_BINS.min(spectrum.len().max(2));
        for i in 0..out {
            let pos = i as f32 / (out - 1).max(1) as f32 * (spectrum.len() - 1) as f32;
            let lo = pos.floor() as usize;
            let hi = (lo + 1).min(spectrum.len() - 1);
            let f = pos - lo as f32;
            let db = spectrum[lo] * (1.0 - f) + spectrum[hi] * f;
            self.bins[i / 4][i % 4] = db;
        }
        self.frame[3] = out as f32;
    }

    /// Add one band's halo. Beyond [`MAX_NODES`] it is dropped, which is the
    /// right failure: the graph cannot show that many bands legibly either.
    pub fn push_node(&mut self, x: f32, y: f32, radius: f32, strength: f32, rgb: [f32; 3]) {
        let i = self.cfg[0] as usize;
        if i >= MAX_NODES {
            return;
        }
        self.nodes[i] = [x, y, radius, strength];
        self.node_color[i] = [rgb[0], rgb[1], rgb[2], 1.0];
        self.cfg[0] = (i + 1) as f32;
    }

    /// Forget the bands, keeping the analyser — called once per frame before
    /// the graph walks its bands again.
    pub fn clear_nodes(&mut self) {
        self.cfg[0] = 0.0;
        self.nodes = [[0.0; 4]; MAX_NODES];
        self.node_color = [[0.0; 4]; MAX_NODES];
    }
}

/// The WGSL this block feeds.
pub const SHADER: &str = include_str!("eq_glow.wgsl");

/// Build the glow surface, if the renderer will give a device.
#[must_use]
pub fn surface(ctx: Box<dyn std::any::Any>) -> Option<ShaderSurface> {
    ShaderSurface::with_uniform_size(ctx, SHADER, std::mem::size_of::<Glow>() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rust's block and the shader's must be the same shape. Every row is a
    /// `vec4`, so the size is a multiple of 16 and nothing is padded.
    #[test]
    fn the_uniform_block_is_vec4_rows() {
        assert_eq!(std::mem::size_of::<Glow>() % 16, 0);
        // frame + cfg + style + 32 bin rows + 24 node rows + 24 colour rows.
        assert_eq!(std::mem::size_of::<Glow>(), (3 + 32 + 24 + 24) * 16);
    }

    /// The shader compiles. Nothing else would notice if it did not: a
    /// surface that fails to build is indistinguishable from a renderer that
    /// declined, so the glow would silently never appear.
    ///
    /// Validated as `compose` assembles it, not on its own: the glow declares
    /// its own uniform binding, and validating the fragment alone passed
    /// happily while the composed module was a redefinition of `u` that no
    /// device would accept.
    #[test]
    fn the_shader_compiles_and_validates() {
        let source = fts_audio_ui::shader::compose(SHADER);
        let module = naga::front::wgsl::parse_str(&source).unwrap_or_else(|e| {
            panic!(
                "the glow shader does not parse: {}",
                e.emit_to_string(&source)
            )
        });
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        );
        if let Err(e) = validator.validate(&module) {
            panic!("the glow shader does not validate: {e:?}");
        }
    }

    /// An analyser of any length lands in the fixed array, and its count is
    /// reported so the shader never reads past what was written.
    #[test]
    fn the_spectrum_resamples_to_the_array() {
        let mut glow = Glow::default();
        // Longer than the array.
        let long: Vec<f32> = (0..512).map(|i| -90.0 + i as f32 / 8.0).collect();
        glow.set_spectrum(&long, -90.0);
        assert_eq!(glow.frame[3], MAX_BINS as f32);
        // Ends are preserved, so the field spans the whole plot.
        assert!((glow.bins[0][0] - long[0]).abs() < 0.5);
        let last = MAX_BINS - 1;
        assert!((glow.bins[last / 4][last % 4] - long[511]).abs() < 0.5);

        // Shorter than the array: it still fills what it can and says how much.
        let short: Vec<f32> = vec![-40.0, -20.0, -30.0];
        glow.set_spectrum(&short, -90.0);
        assert_eq!(glow.frame[3], 3.0);

        // Too short to interpolate is no spectrum at all, not a divide by zero.
        glow.set_spectrum(&[-12.0], -90.0);
        assert_eq!(glow.frame[3], 0.0);
    }

    /// Bands past the ceiling are dropped rather than scribbling over memory.
    #[test]
    fn the_node_array_has_a_floor_and_a_ceiling() {
        let mut glow = Glow::default();
        for i in 0..MAX_NODES + 8 {
            glow.push_node(i as f32, 1.0, 10.0, 1.0, [1.0, 0.0, 0.0]);
        }
        assert_eq!(glow.cfg[0], MAX_NODES as f32);
        glow.clear_nodes();
        assert_eq!(glow.cfg[0], 0.0);
    }
}

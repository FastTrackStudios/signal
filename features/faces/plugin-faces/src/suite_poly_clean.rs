//! Suite `suite-poly-clean`: a boutique amp-sim suite for clean,
//! polyphonic playing — three satin pre-amp pedals, three sculpted amp
//! heads (a pink hammered-glass lead, a white relief rhythm, a black floral
//! acoustic), a 2×12 cab, a pink nine-band tabletop EQ, and three post-amp
//! pedals (a chorus, a big-box delay, a reverb) and a four-voice harmonizer.
//!
//! One page per unit (the view system isn't built yet); each page's root
//! is the unit at its form factor, recorded in the root's name and the file
//! name. Params are namespaced per unit.

mod amps;
mod art;
mod cab;
mod delay;
mod eq;
mod parts;
mod pedals;
mod voices;

use crate::face::Face;

/// The plugin id every page of the suite belongs to.
pub const PLUGIN: &str = "suite-poly-clean";

#[must_use]
pub fn faces() -> Vec<Face> {
    [
        pedals::boost(),
        pedals::compressor(),
        pedals::overdrive(),
        amps::flame_lead(),
        amps::cherub_rhythm(),
        amps::bloom_acoustic(),
        cab::face(),
        eq::face(),
        pedals::chorus(),
        delay::face(),
        pedals::reverb(),
        voices::face(),
    ]
        .into_iter()
        .map(crate::reverb::ui::formatted)
        .collect()
}

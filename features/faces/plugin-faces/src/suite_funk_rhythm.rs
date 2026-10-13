//! Suite: funk rhythm — a clean-funk guitar rig as one plugin
//! (`suite-funk-rhythm`), each unit a page in its own form factor
//! (`docs/form-factors.md`): the treadle wah, four standard pre-FX
//! stompboxes, three amp heads, the cab, the desktop graphic EQ and three
//! post-FX pedals (one a big-box delay).
//!
//! The lettering follows the style of the rig it recreates — heavy
//! lowercase names stacked in a pedal's corner, a cartoon maker's mark on
//! the amps — with every name invented.

mod amps;
mod cab;
mod delay;
mod eq;
pub mod parts;
mod pedals;
mod wah;

use crate::face::Face;

/// Every unit of the suite, in signal order.
#[must_use]
pub fn faces() -> Vec<Face> {
    vec![
        wah::face(),
        pedals::envelope(),
        pedals::squeeze(),
        pedals::glass(),
        pedals::haul(),
        amps::critic(),
        amps::engine(),
        amps::desk(),
        cab::face(),
        eq::face(),
        pedals::neon(),
        delay::face(),
        pedals::rinse(),
    ]
}

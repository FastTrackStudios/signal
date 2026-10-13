//! Suite: boutique clean — a soft pastel boutique rig as one plugin
//! (`suite-boutique-clean`), each unit a page in its own form factor
//! (`docs/form-factors.md`): four standard pre-FX stompboxes, three amp
//! heads, the cab, the desktop graphic EQ, and the echo and reverb rack
//! units of the post-FX stand.
//!
//! The lettering follows the style of the rig it recreates — block caps
//! between chevrons on the pedals, a brush-drawn mark where the rig signs
//! its gear — with every name and mark invented.

mod amps;
mod cab;
mod eq;
mod post;
pub mod look;
mod pedals;

use crate::face::Face;

/// Every unit of the suite, in signal order.
#[must_use]
pub fn faces() -> Vec<Face> {
    vec![pedals::comp(), pedals::od1(), pedals::od2(), pedals::modulation(), amps::blush(), amps::ranch(), amps::kintsugi(), cab::face(), eq::face(), post::echo(), post::reverb()]
}

//! Suite: studio blues — a clean-to-crunch guitar rig recreated as frame
//! surfaces, one page per unit, in the order the signal meets them:
//!
//! - pre-amp pedals (hand-painted boutique boxes): a clean boost, an
//!   envelope filter, a vintage overdrive, a transparent overdrive, a short
//!   delay
//! - three amps: a boutique clean head, a lead head, a two-channel combo
//! - a tabletop harmonic-tremolo and spring-reverb unit
//! - an EQ and a compressor on the desk
//! - a 1×12 cab with two mics and the room
//! - post-amp pedals: a double-wide delay and a studio reverb
//!
//! Every unit binds its own namespace in the `suite-studio-blues` catalog.

pub mod amps;
pub mod cab;
mod desk;
pub mod parts;
pub mod pedals;
pub mod tank;

use crate::face::Face;

pub use parts::PLUGIN;

#[must_use]
pub fn faces() -> Vec<Face> {
    vec![
        pedals::plain_boost(),
        pedals::peak_filter(),
        pedals::mild_drive(),
        pedals::teal_drive(),
        pedals::short_delay(),
        pedals::haze_delay(),
        amps::clean_head(),
        amps::lead_head(),
        amps::silk_combo(),
        tank::spring_tank(),
        pedals::plate_verb(),
        desk::eq(),
        desk::comp(),
        cab::cab(),
    ]
    .into_iter()
    .map(crate::reverb::ui::formatted)
    .collect()
}

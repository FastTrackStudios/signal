//! Knobs drawn from photographs of particular classic studio units — the
//! scalloped phenolic of a passive program equalizer, a large-format
//! console's coloured caps, a proportional-Q module's winged concentric
//! pair, and a class-A module's turrets. Each follows `KIT.md`: the body
//! that turns is a square frame round the shaft, the room's light stays put.

mod cap;
mod parts;
mod scalloped;
mod turret;
mod winged;

use crate::surface::kit::registry::ComponentDef;

/// This group's components.
pub(crate) fn defs() -> Vec<ComponentDef> {
    let mut all = scalloped::defs();
    all.extend(cap::defs());
    all.extend(winged::defs());
    all.extend(turret::defs());
    all
}

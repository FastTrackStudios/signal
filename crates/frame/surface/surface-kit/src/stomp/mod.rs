//! Modern boutique pedal and amp parts: teardrop, dot and fluted knobs, a
//! painted stompbox enclosure, a status LED set apart from its footswitch,
//! and a lit graphic-EQ fader.

pub(crate) mod brass;
pub(crate) mod enclosure;
pub(crate) mod flat;
pub(crate) mod knobs;
pub(crate) mod lamps;

use crate::registry::ComponentDef;

/// This module's components.
pub(crate) fn defs() -> Vec<ComponentDef> {
    let mut v = knobs::defs();
    v.extend(enclosure::defs());
    v.extend(flat::defs());

    v.extend(lamps::defs());
    v.extend(brass::defs());
    v
}

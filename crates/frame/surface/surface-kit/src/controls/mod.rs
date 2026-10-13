//! Kit families: buttons, switches and levers — the panel's switching.
//! Ported from `fts-audio-ui::hardware` (`button`, `button_kit`,
//! `button_parts`, `switches`, `lever`) — see `KIT.md`.

mod bare_toggle;
mod buttons;
mod eq_slider;
mod key_column;
mod lamp_rocker;
mod levers;
pub(crate) mod parts;
mod piano;
mod stomp;
mod stomp_box;
mod stomp_flat;
mod studio;
mod switches;

use crate::registry::ComponentDef;

/// This family's components.
pub(crate) fn defs() -> Vec<ComponentDef> {
    let mut all = buttons::defs();
    all.extend(switches::defs());
    all.extend(levers::defs());
    all.extend(key_column::defs());
    all.extend(studio::defs());
    all.extend(bare_toggle::defs());
    all.extend(piano::defs());
    all.extend(stomp::defs());
    all.extend(eq_slider::defs());
    all.extend(stomp_flat::defs());
    all.extend(lamp_rocker::defs());
    all.extend(stomp_box::defs());
    all
}

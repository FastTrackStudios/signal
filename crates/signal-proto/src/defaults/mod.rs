//! Default templates — structural blueprints for common rig configurations.

pub mod archetype_jm;
pub mod archetype_ndsp;
pub mod guitar;

pub use archetype_jm::archetype_john_mayer;
pub use archetype_ndsp::{
    archetype_cory_wong_x, archetype_john_mayer_x_full, archetype_label, archetype_misha_mansoor_x,
    archetype_nolly_x, archetype_petrucci_x, archetype_rabea_x, archetype_seed_slug,
    archetype_tim_henson_x, archetype_x_templates, NDSP_ARCHETYPE_X_PLUGIN_NAMES,
};
pub use guitar::guitar_rig_template;

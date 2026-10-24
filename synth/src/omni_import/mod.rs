//! **Omnisphere patch import.**
//!
//! Parse Spectrasonics `.prt_omn` patch files (the "`AmberPart`" XML dialect)
//! and map them onto the composition tree from [`crate::omni`], realizing
//! Soundsource blocks against the local soundsource extraction by name.
//!
//! Format notes (reverse-engineered from the Settings Library):
//! - Plain XML, no declaration; root `<AmberPart><SynthEngine>…`.
//! - Numeric attributes are IEEE-754 `f32` bit patterns as 8 hex digits
//!   (`3f800000` = 1.0); small integers are written as decimal.
//! - `SYNTHENG` holds `VOICE` (one per layer: FILTER, WAVESHAPER, OSC + HARM,
//!   AENV/FENV + params, a 4-slot `EFFRACK`), a parallel `MULTISAMPLE` list
//!   (its `MS_IM_0 name= library=` names the layer's soundsource), the
//!   common `EFFRACK` + `AUXEFFRACK`, `LFO_SET` (6 LFOs), 6 `MODENV`s and
//!   the flat-attribute `MOD_MATRIX` (`source0`/`target0`/…).
//! - `ENTRYDESCR` carries the patch name, library and the browser tag string
//!   (`Author=…;Genre=…;Mood=…`).

mod xml;

pub use xml::{XmlNode, omni_num, parse_xml};
pub mod effects;
mod index;
mod model;
mod multi;
pub mod state;
mod tree;

pub use index::{SoundsourceIndex, normalize_soundsource_name, resolve_name};
pub use model::{
    FilterModel, OmniLayer, OmniLfo, OmniModEnv, OmniModRoute, OmniPatch, classify_filter_full,
    filter_model, omni_cutoff_hz, omni_filter_setting, parse_patch, wavetable_path,
};
pub use multi::{OmniMulti, load_multi_file, multi_to_container, parse_multi};
pub use tree::{LAYER_NAMES, load_patch_file, load_patch_file_transposed, patch_to_container};

#[cfg(test)]
use model::classify_effect;

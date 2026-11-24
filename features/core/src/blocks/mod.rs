//! Signal's own blocks.
//!
//! The effects are the plugins' own, headless (`plugin_processors`, from
//! FastTrackStudios/plugins, on FastTrackStudios/dsp): the `eq` block is
//! the FTS EQ's processor, the `comp` block the compressor's, and so on,
//! so a chain sounds the same in Signal as with the plugins one by one.
//! What is Signal's alone are the rig's pieces (NAM and the cab; the
//! cab's FIR is an algorithm and belongs in `dsp` next):
//!
//! | name     | from                                    | needs        |
//! |----------|-----------------------------------------|--------------|
//! | `gain`, `gate`, `drive`, `eq`, `comp`, `delay`, `reverb` | `plugin_processors` | |
//! | `nam`    | a NAM capture (`neural-amp-modeler`)    | feature `nam`, a file |
//! | `cab`    | a cabinet IR (direct FIR)               | a file       |
//!
//! Every one runs everywhere the core builds.

use alloc::boxed::Box;

use crate::block::{BlockProcessor, BuildContext, Unavailable};
use crate::state::BlockSpec;

pub mod cab;
#[cfg(feature = "nam")]
pub mod nam;

/// Signal's rig blocks, beside the plugins' effects.
const RIG_BLOCKS: &[&str] = &[
    #[cfg(feature = "nam")]
    "nam",
    "cab",
];

/// Whether this build makes block `name`.
#[must_use]
pub fn exists(name: &str) -> bool {
    RIG_BLOCKS.contains(&name) || plugin_processors::NAMES.contains(&name)
}

/// Every block name this build makes.
pub fn names() -> impl Iterator<Item = &'static str> {
    plugin_processors::NAMES.iter().chain(RIG_BLOCKS).copied()
}

/// Build Signal's block `name` for `spec`, compiled in.
///
/// # Errors
///
/// [`Unavailable`] when this build doesn't make it or its file is missing.
pub fn build(
    name: &str,
    spec: &BlockSpec,
    cx: &BuildContext<'_>,
) -> Result<Box<dyn BlockProcessor>, Unavailable> {
    match name {
        #[cfg(feature = "nam")]
        "nam" => Ok(Box::new(nam::Nam::from_bytes(
            &cx.asset_bytes(spec)?,
            cx.sample_rate,
            cx.max_block,
        )?)),
        "cab" => Ok(Box::new(cab::Cab::from_wav(&cx.asset_bytes(spec)?)?)),
        _ => match plugin_processors::build(name, cx.sample_rate, cx.max_block) {
            Some(Ok(p)) => Ok(p),
            Some(Err(e)) => Err(Unavailable::Failed {
                why: alloc::format!("{e}"),
            }),
            None => Err(Unavailable::UnknownKind),
        },
    }
}

//! Signal's own blocks.
//!
//! The effects are families (`plugin_processors`, from
//! FastTrackStudios/plugins, on FastTrackStudios/dsp): one `eq` block, one
//! `reverb` block, each a folder of algorithms over compiled-in engines.
//! The block's `algorithm` picks one (an EQ face, a reverb's space), and
//! its parameters are that algorithm's controls. Algorithms are data, so
//! a downloaded one runs without rebuilding Signal.
//! What is Signal's alone are the rig's pieces (NAM and the cab; the
//! cab's FIR is an algorithm and belongs in `dsp` next):
//!
//! | name     | from                                    | needs        |
//! |----------|-----------------------------------------|--------------|
//! | `gain`, `gate`, `drive`, `eq`, `comp`, `delay`, `reverb` | `plugin_processors` | |
//! | `nam`    | a NAM capture (`neural-amp-modeler`)    | feature `nam`, a file |
//! | `cab`    | a cabinet IR (direct FIR)               | a file       |
//! | `sample` | a one-shot WAV played by hits           | a file       |
//! | `level`  | offline: the vocal leveler's four lanes ([`crate::offline::level`]) | an analysis |
//! | `align`  | offline: delay, polarity, phase rotation ([`crate::offline::align`]) | an analysis |
//!
//! Every one runs everywhere the core builds.

use alloc::boxed::Box;

use crate::block::{BlockProcessor, BuildContext, Unavailable};
use crate::state::BlockSpec;

pub mod cab;
#[cfg(feature = "nam")]
pub mod nam;
pub mod sample;

/// Signal's rig blocks, beside the plugins' effects.
const RIG_BLOCKS: &[&str] = &[
    #[cfg(feature = "nam")]
    "nam",
    "cab",
    "sample",
    crate::offline::level::NAME,
    crate::offline::align::NAME,
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
        "sample" => Ok(Box::new(sample::Sample::from_wav(&cx.asset_bytes(spec)?)?)),
        crate::offline::level::NAME => Ok(Box::new(crate::offline::level::LevelApply::new(
            cx.sample_rate,
        ))),
        crate::offline::align::NAME => Ok(Box::new(crate::offline::align::AlignApply::new(
            cx.sample_rate,
            spec.params
                .get(crate::offline::align::PARAMS[crate::offline::align::DELAY].id)
                .copied()
                .unwrap_or(0.0),
        ))),
        // A family on the spec's algorithm. Families need the registry,
        // which needs `std`.
        #[cfg(not(feature = "std"))]
        _ => {
            let _ = spec;
            Err(Unavailable::UnknownKind)
        }
        #[cfg(feature = "std")]
        _ => match plugin_processors::build_algorithm(
            name,
            spec.algorithm.as_deref(),
            cx.sample_rate,
            cx.max_block,
        ) {
            Some(Ok(p)) => Ok(p),
            Some(Err(e)) => Err(Unavailable::Failed {
                why: alloc::format!("{e}"),
            }),
            // The family is here but not this algorithm (a downloaded one
            // this machine doesn't have).
            None if plugin_processors::NAMES.contains(&name) => Err(Unavailable::Missing {
                what: alloc::format!(
                    "{name} algorithm {}",
                    spec.algorithm.as_deref().unwrap_or("(default)")
                ),
            }),
            None => Err(Unavailable::UnknownKind),
        },
    }
}

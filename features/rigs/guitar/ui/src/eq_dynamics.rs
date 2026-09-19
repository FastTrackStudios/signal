//! The EQ's dynamics and spectral controls — the processor repo's own panel,
//! bound to the wire.
//!
//! # What was missing
//!
//! The EQ surface exposed six fields per band: `used`, `on`, `freq`, `gain`,
//! `q`, `shape`. The DSP has twenty-two. Everything that makes a band
//! *dynamic* — range, threshold, attack, release, auto, the spectral toggle
//! and its tilt and density — was on the wire already (`param_specs` sends
//! the EQ's whole surface) and simply never drawn, which is the same gap
//! `eq_ui::dynamics` was written upstream to close for the plugin.
//!
//! So this draws no controls of its own. It binds the band's parameters
//! through [`WireParam`] and mounts
//! [`BandDynamicsPanel`](eq_ui::dynamics::BandDynamicsPanel) — the same
//! widget the plugin editor uses, with the same ring, badge and expansion
//! behaviour. A detached remote should be the same editor, not a smaller one.
//!
//! # What it cannot show yet
//!
//! `DynState::live_db` is where dynamics have pushed the band *this instant*,
//! published by the audio thread. The rig does not put it on the wire, so the
//! live arc reads zero and the ring shows only the range. The panel handles
//! that as the idle case, which is what it looks like on a band that is not
//! being driven — wrong only while audio is moving. Sending it means a new
//! meter-rate field, which is its own piece of work.

use dioxus::prelude::*;
use eq_ui::dynamics::{BandDynamicsPanel, DynState};
use fts_audio_ui::prelude::ParamHandle;
use signal_guitar_proto::LiveBlock;

use crate::wire_param::{EditSink, WireParam};

/// The dynamics panel for one band of an EQ block.
///
/// Renders nothing when the block does not report the band's dynamics
/// parameters — an EQ that predates them, or a different EQ entirely.
///
/// A plain function rather than a `#[component]`: every Dioxus prop must be
/// `PartialEq`, and an [`EditSink`] is a closure. The panel it mounts *is* a
/// component; this is the binding in front of it.
#[must_use]
pub fn band_dynamics(block: &LiveBlock, band: usize, sink: &EditSink) -> Element {
    // One-based on the wire: `b1_…` is the first band.
    let param = |suffix: &str| format!("b{}_{suffix}", band + 1);
    let bind =
        |suffix: &str| WireParam::bind(sink.clone(), block, &param(suffix)).map(WireParam::handle);

    // The range parameter is the one that decides whether a band is dynamic
    // at all, so its absence is what "this EQ has no dynamics" means.
    let Some(range) = bind("dyn_range") else {
        return rsx! {};
    };
    let range_max_db = block
        .params
        .iter()
        .find(|p| p.name == param("dyn_range"))
        .map_or(30.0, |p| p.max);

    // A control the DSP does not report draws inert rather than absent: the
    // panel is the specification for what a band can do, and a missing knob
    // reads as a missing feature rather than an unfinished one.
    let or_inert = |suffix: &str, position: f32| {
        bind(suffix).unwrap_or_else(|| ParamHandle::inert(param(suffix), position))
    };

    let state = DynState {
        range,
        spectral: or_inert("spectral", 0.0),
        tilt: or_inert("spectral_tilt", 0.0),
        // Not on the wire — see the module docs.
        live_db: 0.0,
        range_max_db,
    };

    rsx! {
        BandDynamicsPanel {
            state,
            threshold: or_inert("dyn_thr", 0.5),
            attack: or_inert("dyn_atk", 0.2),
            release: or_inert("dyn_rel", 0.3),
            auto: or_inert("dyn_auto", 1.0),
            density: or_inert("spectral_density", 0.5),
        }
    }
}

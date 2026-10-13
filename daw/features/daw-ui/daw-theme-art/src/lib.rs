//! Theme artwork as Dioxus components.
//!
//! REAPER's chrome ships as ~1000 PNGs. Recolouring them (see
//! `fts_themer::restyle`) moves them onto a palette but leaves the *shapes*
//! inherited — and gives the web GUI nothing, since it would have to load
//! pictures of a mixer strip rather than draw one.
//!
//! So the artwork is authored once, as Dioxus components emitting SVG, and
//! consumed twice:
//!
//! ```text
//!            #[component] fn Button(..) -> Element   (rsx! → <svg>)
//!                          │
//!            ┌─────────────┴──────────────┐
//!            ▼                            ▼
//!   web GUI: rendered live        REAPER: rasterised to PNG
//!   (vector, any DPI, animatable)  at 100 / 150 / 200 %
//! ```
//!
//! The same function draws the button you click in the browser and the
//! button REAPER blits into the mixer.
//!
//! # Scope
//!
//! 571 of REAPER's 1021 images are toolbar icons, which `fts-icons` already
//! generates from SVG at all three scales — that half needs a *spec*, not a
//! renderer. What is left is ~450 pieces of chrome, and most of it is a few
//! shapes in many states, so this is a few dozen parameterised components
//! rather than 450 drawings.
//!
//! # Why rasterise at all
//!
//! REAPER cannot load SVG. The PNGs are build output from here on, which is
//! the point: change the palette or a component and every scale regenerates.

pub mod art_data;
pub mod collapse;
pub mod components;
pub mod dress;
pub mod generated;
pub mod geometry;
pub mod mixer_controls;
pub mod primitives;
pub mod slice;
pub mod strip;
pub mod vector_controls;

// Everything that reads or writes a bitmap. The components themselves need
// none of it, and the app that renders them as live SVG should not have to
// build `image` and `resvg` to do so — which is what `default-features =
// false` is for, and what these gates make true. Without them the crate
// only ever compiled one way, and the "just the components" build the
// manifest advertises did not exist.
#[cfg(feature = "render")]
pub mod compare;
#[cfg(feature = "render")]
pub mod derive;
#[cfg(feature = "render")]
pub mod export;
#[cfg(feature = "render")]
pub mod render;
#[cfg(feature = "render")]
pub mod trace;

pub use art_data::{ArtData, ArtImage, ColorMode, Rect};
// `Band` is deliberately *not* re-exported: `vector_controls::Band` is a
// plate's colour band and already lives at that name. The stretch band stays
// spelled `slice::Band`, which is where its meaning is legible.
#[cfg(feature = "render")]
pub use compare::{Fidelity, compare};
#[cfg(feature = "render")]
pub use derive::DerivedSpec;
pub use slice::{NamedArt, Slice};
#[cfg(feature = "render")]
pub use trace::trace;

#[cfg(feature = "render")]
pub use export::{cell_markup, composite_cells, generatable, render_control};
pub use mixer_controls::{
    FxButton, FxChain, InputMonitorIndicator, Monitoring, MuteButton, PanningKnob, RecordArm,
    RecordArmButton, RoutingButton, Solo, SoloButton, VolumeFaderCap, VolumeFaderTrack,
};
pub use primitives::{Button, ControlState, Groove, Meter, Panel, Thumb};
#[cfg(feature = "render")]
pub use render::{RenderError, render_for, render_sized, render_svg};
pub use strip::{Mixer, MixerStrip};

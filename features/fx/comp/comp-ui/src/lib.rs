//! Compressor GUI — Dioxus components for the compressor plugin.
//!
//! - Control view: transfer curve, gain reduction meter, waveform display,
//!   inline parameter sliders
//! - Profile views: rendered from profile definitions (1176, LA-2A, SSL, etc.)

pub mod control_view;
pub mod profile_view;

pub use control_view::{CompSlider, GrMeter, LevelMeter, TransferCurve, WaveformDisplay};
pub use profile_view::{
    profile_skin, ProfileControlGroup, ProfileControlKind, ProfileControlView, ProfileParamWrite,
    ProfileSkin, ProfileSkinGroup, ProfileView,
};

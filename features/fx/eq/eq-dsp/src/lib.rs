#![allow(rustdoc::broken_intra_doc_links)]
// The library has a large amount of code transliterated from a reference
// binary; the following style lints fire heavily on that code without
// actually flagging bugs. Allow them at the crate level rather than
// scattering #[allow(...)] attributes through every helper.
#![allow(
    clippy::manual_clamp,
    clippy::doc_lazy_continuation,
    clippy::doc_overindented_list_items,
    clippy::empty_line_after_doc_comments,
    clippy::empty_line_after_outer_attr,
    clippy::if_same_then_else,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::needless_range_loop,
    clippy::assertions_on_constants
)]

//! Filter design and biquad-cascade DSP for the FTS-EQ plugin.
//!
//! Pipeline:
//!   1. Analog prototype (Butterworth pole/zero geometry).
//!   2. Frequency transformation (LP→BP via elliptic functions, LP→BS, bilinear).
//!   3. Per-section synth (universal-synth helper covers shelves, bell, allpass).
//!   4. Bilinear transform + biquad assembly.
//!
//! Filter types ([`design::FilterType`]):
//!   - `Peak` / Bell, `Highpass`, `Lowpass`, `Bandpass`, `Notch`,
//!     `BandPassVariant`, `FlatTilt`, `LowShelf`, `HighShelf`, `TiltShelf`,
//!     `BandShelf`, `Allpass`, `ShelfAlt`.

pub mod band;
pub mod biquad;
pub mod calibration;
pub mod cascade;
pub mod chain;
pub mod constants;
pub mod delay;
pub mod design;
pub mod dynamics;
pub mod elliptic;
pub mod hardware_eq;
pub mod hardware_targets;
pub mod neve_1073;
pub mod parameters;
pub mod proq4_mzt;
pub mod proq4_peak;
pub mod proq4_per_section_helpers;
pub mod prototype;
pub mod response;
pub mod section;
pub mod shelf;
pub mod shelf_zpk;
pub mod slope;
pub mod spectral;
pub mod transform;
pub mod transient;
pub mod zpk;

pub use band::Band;
pub use chain::EqChain;
pub use design::FilterType;

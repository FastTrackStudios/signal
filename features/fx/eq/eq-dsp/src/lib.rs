#![expect(rustdoc::broken_intra_doc_links, reason = "large transliterated codebase may have incomplete intra-doc links")]
// The library has a large amount of code transliterated from a reference
// binary; the following style lints fire heavily on that code without
// actually flagging bugs. They are suppressed via item-level attributes
// rather than at the crate level.

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

// Realtime guard. This crate runs on an audio callback, so the calls in
// clippy.toml's disallowed-methods list (locks, env, sleep) are real bugs here
// even though they are allowed workspace-wide off the audio thread.
#![deny(clippy::disallowed_methods)]

// ── The layers, outermost first ──────────────────────────────────────────
//
// The split that matters is `design` against `runtime`: design is arithmetic
// that happens when a parameter changes and may be as expensive as it likes,
// runtime is arithmetic that happens per sample and may not allocate, lock or
// branch unpredictably. Everything else follows from that one line.

/// The engine every front end drives — the plugin and the rig's EQ block.
pub mod engine;

/// Coefficient design: parameters in, biquad cascades out. Not realtime.
pub mod design;
/// The per-sample path: sections, bands, the chain, and response readout.
pub mod runtime;
/// Level detection and the dynamic/spectral/transient bands built on it.
pub mod dynamics;

/// Textbook filter mathematics, with no Pro-Q in it.
pub mod math;
/// Fixed response curves modelled from named analogue units.
pub mod hardware;

pub use design::FilterType;
pub use runtime::band::Band;
pub use runtime::chain::EqChain;

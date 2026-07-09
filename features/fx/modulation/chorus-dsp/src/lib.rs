//! FTS Chorus — multi-engine chorus/flanger/vibrato.
//!
//! Five distinct chorus engines covering clean to experimental:
//! - **Cubic**: Clean Catmull-Rom interpolation (default, transparent)
//! - **BBD**: Bucket-brigade device emulation (vintage analog)
//! - **Tape**: Wow/flutter/saturation (warm tape character)
//! - **Orbit**: Dual-tap elliptical orbital modulation (experimental, spatial)
//! - **Juno**: Triangle LFO + allpass interpolation (classic Roland Juno-60)
//!
//! Each engine can operate in Chorus, Flanger, or Vibrato mode.
//!
//! Credits:
//! - Cubic interpolation: standard Catmull-Rom (fts-dsp)
//! - BBD topology: Choroboros (EsotericShadow), clock-driven S&H chain
//! - Tape modulation: ChowDSP AnalogTapeModel (wow/flutter), qdelay (tiagolr)
//! - Orbit modulation: Choroboros (EsotericShadow), elliptical 2D LFO
//! - Juno: TAL-NoiseMaker / YKChorus (SpotlightKid), allpass delay + DC block

pub mod chain;
pub mod engine;

//! FTS Pitch — Real-time pitch shifting DSP.
//!
//! Four algorithm modes, selectable at runtime:
//!
//! 1. **FreqDivider** — Analog-style frequency division (Boss OC-2).
//!    Zero latency, synthy square-wave character.
//!
//! 2. **PLL** — Phase-locked loop tracking oscillator.
//!    Zero latency, warmer sub with saw/triangle waveforms.
//!
//! 3. **Granular** — Fixed-ratio granular pitch shifter.
//!    ~512–1024 sample latency, natural tone.
//!
//! 4. **PSOLA** — Pitch-synchronous overlap-add.
//!    ~1152 sample latency, highest quality monophonic.
//!
//! 5. **Spectral** — phase-locked phase vocoder (Laroche–Dolson peak
//!    shifting). `fft_size` latency (1024–8192), polyphonic, no grain
//!    warble — the engine for shifters inside delay/reverb feedback loops.

pub mod allpass_shift;
pub mod chain;
pub mod divider;
pub mod granular;
pub mod pll;
pub mod pog;
pub mod psola;
#[cfg(feature = "rubberband")]
pub mod rubberband;
pub mod spectral;
pub mod unison;
pub mod wsola;

//! Loud slew-rate limiter.
//!
//! Limits the rate of change (slew rate) of the signal to tame
//! inter-sample peaks. Operates on the derivative of the waveform
//! rather than its amplitude.

// TODO: Implement Loud algorithm from Airwindows

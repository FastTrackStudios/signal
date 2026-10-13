//! Filter mathematics, with no Pro-Q in it.
//!
//! Everything here is textbook: a zero-pole-gain representation, the analog
//! prototypes, the elliptic functions that place bandpass and bandstop poles,
//! and the frequency transforms between them. None of it knows what an EQ band
//! is, and none of it should — that is what makes it checkable against a
//! reference book rather than against a binary.

pub mod elliptic;
pub mod prototype;
pub mod transform;
pub mod zpk;

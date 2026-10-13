//! Libraries: shared surface libraries — protocol, filesystem store and the live link.

#[cfg(feature = "library")]
pub mod proto;

#[cfg(feature = "library")]
pub mod fs;

#[cfg(feature = "library")]
pub mod link;

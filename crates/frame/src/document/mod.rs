//! Documents: the frame document model, its text form, file IO, bundles and the Loro CRDT store.

pub mod proto;

pub mod text;

#[cfg(feature = "io")]
pub mod io;

#[cfg(feature = "io")]
pub mod bundle;

#[cfg(feature = "io")]
pub mod loro;

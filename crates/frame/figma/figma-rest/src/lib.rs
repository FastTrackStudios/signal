//! Figma importer facade for frame-proto.
//!
//! This crate intentionally stays thin: parsing and flattening logic lives in
//! `frame-proto`, and this crate provides stable import entry points.

pub mod converter;
pub mod error;

pub use converter::{
    import_figma_bytes, import_figma_bytes_with_diagnostics, import_figma_file, import_get_file,
    ImportDiagnostics,
};
pub use error::ImportError;

//! Dynamic Template Protocol
//!
//! This crate defines the data types and service interfaces for dynamic template
//! organization. It provides DAW-agnostic types for organizing audio items into
//! hierarchical track structures.
//!
//! # Overview
//!
//! The dynamic template system takes a list of item names (e.g., audio file names)
//! and organizes them into a logical track hierarchy based on pattern matching
//! against known instrument groups (Drums, Bass, Guitars, etc.).
//!
//! # Example
//!
//! ```ignore
//! use dynamic_template_proto::{OrganizeRequest, OrganizeOptions};
//!
//! let request = OrganizeRequest {
//!     items: vec![
//!         "Kick In.wav".to_string(),
//!         "Kick Out.wav".to_string(),
//!         "Snare Top.wav".to_string(),
//!         "Bass DI.wav".to_string(),
//!     ],
//!     existing_tracks: None,
//!     options: OrganizeOptions::default(),
//! };
//!
//! // The service would organize these into:
//! // [F] Drums
//! //   [F] Kick
//! //     Kick In
//! //     Kick Out
//! //   Snare Top
//! // [F] Bass
//! //   Bass DI
//! ```

#![deny(unsafe_code)]

mod error;
mod services;
pub mod session_template;
mod types;
pub mod visibility_manager;

pub use error::*;
pub use services::*;
pub use session_template::{
    BusMaterialization, GroupMembership, IdealFullSessionTemplate, MonitorMode, NodeKind,
    NodeRouting, TemplateBus, TemplateNode, TrackDefaults, TrackInput,
};
pub use types::*;

// Re-export daw-proto hierarchy types for convenience
pub use daw_proto::{FolderDepthChange, TrackHierarchy, TrackHierarchyBuilder, TrackNode};

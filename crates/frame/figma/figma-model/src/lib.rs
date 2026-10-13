//! Frame Protocol — Figma-compatible vector design document model.
//!
//! This crate exposes a rendering-friendly IR that stays close to Figma while
//! preserving full-fidelity source data for round-tripping and future targets.

pub mod color;
pub mod diff;
pub mod document;
pub mod effect;
pub mod figma;
pub mod geometry;
pub mod history;
pub mod id;
pub mod layout;
pub mod node;
pub mod paint;
pub mod projection;
pub mod typography;
pub mod vector;

pub use color::Color;
pub use diff::{DocumentDiff, DocumentOp};
pub use document::{AssetData, AssetKind, FrameDocument, StyleDefinition};
pub use effect::Effect;
pub use figma::{FigmaDocumentMeta, FigmaNodeData, FigmaPayload};
pub use geometry::{CornerRadii, Point, Size, Transform};
pub use history::DocumentHistory;
pub use id::*;
pub use layout::*;
pub use node::{
    ArcData, ExportConstraintType, ExportFormat, ExportSetting, FrameNode, NodeKind,
    OverrideProperty, PropertyOverride,
};
pub use paint::*;
pub use projection::{
    classify_node_type, get_auto_layout_projection, project_node, RenderNodeClass,
    RenderNodeProjection, RenderTextProjection,
};
pub use typography::*;
pub use vector::*;

//! Generic override path/op model.
//!
//! Overrides let higher-level variants (Layer, Engine, Rig, Song sections)
//! modify lower-level parameters without changing the referenced variant directly.

use facet::Facet;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::ParameterValue;

// ─── Override target path ───────────────────────────────────────

/// Strongly-typed segment inside an override path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum NodePathSegment {
    Engine(String),
    Layer(String),
    Module(String),
    Block(String),
    Parameter(String),
    Raw(String),
}

impl NodePathSegment {
    fn to_legacy_piece(&self) -> String {
        match self {
            Self::Engine(id) => format!("engine.{id}"),
            Self::Layer(id) => format!("layer.{id}"),
            Self::Module(id) => format!("module.{id}"),
            Self::Block(id) => format!("block.{id}"),
            Self::Parameter(id) => format!("param.{id}"),
            Self::Raw(raw) => raw.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodePathError {
    Empty,
    MissingSegmentId { kind: String },
    UnknownKind(String),
}

impl fmt::Display for NodePathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "path is empty"),
            Self::MissingSegmentId { kind } => write!(f, "missing id after '{kind}'"),
            Self::UnknownKind(kind) => write!(f, "unknown path segment kind '{kind}'"),
        }
    }
}

impl std::error::Error for NodePathError {}

/// Typed override path for traversal across Engine/Layer/Module/Block/Parameter.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Facet)]
pub struct NodePath(pub Vec<NodePathSegment>);

impl NodePath {
    pub fn new(segments: Vec<NodePathSegment>) -> Self {
        Self(segments)
    }

    pub fn segments(&self) -> &[NodePathSegment] {
        &self.0
    }

    pub fn engine(id: impl Into<String>) -> Self {
        Self(vec![NodePathSegment::Engine(id.into())])
    }

    pub fn layer(id: impl Into<String>) -> Self {
        Self(vec![NodePathSegment::Layer(id.into())])
    }

    pub fn module(id: impl Into<String>) -> Self {
        Self(vec![NodePathSegment::Module(id.into())])
    }

    pub fn block(id: impl Into<String>) -> Self {
        Self(vec![NodePathSegment::Block(id.into())])
    }

    pub fn parameter(id: impl Into<String>) -> Self {
        Self(vec![NodePathSegment::Parameter(id.into())])
    }

    #[must_use]
    pub fn with_engine(mut self, id: impl Into<String>) -> Self {
        self.0.push(NodePathSegment::Engine(id.into()));
        self
    }

    #[must_use]
    pub fn with_layer(mut self, id: impl Into<String>) -> Self {
        self.0.push(NodePathSegment::Layer(id.into()));
        self
    }

    #[must_use]
    pub fn with_module(mut self, id: impl Into<String>) -> Self {
        self.0.push(NodePathSegment::Module(id.into()));
        self
    }

    #[must_use]
    pub fn with_block(mut self, id: impl Into<String>) -> Self {
        self.0.push(NodePathSegment::Block(id.into()));
        self
    }

    #[must_use]
    pub fn with_parameter(mut self, id: impl Into<String>) -> Self {
        self.0.push(NodePathSegment::Parameter(id.into()));
        self
    }

    /// Legacy dot-path rendering for logs/tests/UI.
    pub fn as_str(&self) -> String {
        self.0
            .iter()
            .map(NodePathSegment::to_legacy_piece)
            .collect::<Vec<_>>()
            .join(".")
    }

    /// Best-effort parser for legacy dot-paths.
    pub fn parse_legacy(path: &str) -> Self {
        let tokens: Vec<&str> = path.split('.').collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            let tok = tokens[i];
            let next = tokens.get(i + 1).copied();
            match (tok, next) {
                ("engine", Some(id)) => {
                    out.push(NodePathSegment::Engine(id.to_string()));
                    i += 2;
                }
                ("layer", Some(id)) => {
                    out.push(NodePathSegment::Layer(id.to_string()));
                    i += 2;
                }
                ("module", Some(id)) => {
                    out.push(NodePathSegment::Module(id.to_string()));
                    i += 2;
                }
                ("block", Some(id)) => {
                    out.push(NodePathSegment::Block(id.to_string()));
                    i += 2;
                }
                ("param" | "parameter", Some(id)) => {
                    out.push(NodePathSegment::Parameter(id.to_string()));
                    i += 2;
                }
                _ => {
                    out.push(NodePathSegment::Raw(tok.to_string()));
                    i += 1;
                }
            }
        }
        Self(out)
    }

    pub fn try_parse_legacy(path: &str) -> Result<Self, NodePathError> {
        if path.trim().is_empty() {
            return Err(NodePathError::Empty);
        }
        let tokens: Vec<&str> = path.split('.').collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            let kind = tokens[i];
            let id = tokens.get(i + 1).copied();
            match kind {
                "engine" => out.push(NodePathSegment::Engine(
                    id.ok_or(NodePathError::MissingSegmentId {
                        kind: kind.to_string(),
                    })?
                    .to_string(),
                )),
                "layer" => out.push(NodePathSegment::Layer(
                    id.ok_or(NodePathError::MissingSegmentId {
                        kind: kind.to_string(),
                    })?
                    .to_string(),
                )),
                "module" => out.push(NodePathSegment::Module(
                    id.ok_or(NodePathError::MissingSegmentId {
                        kind: kind.to_string(),
                    })?
                    .to_string(),
                )),
                "block" => out.push(NodePathSegment::Block(
                    id.ok_or(NodePathError::MissingSegmentId {
                        kind: kind.to_string(),
                    })?
                    .to_string(),
                )),
                "param" | "parameter" => out.push(NodePathSegment::Parameter(
                    id.ok_or(NodePathError::MissingSegmentId {
                        kind: kind.to_string(),
                    })?
                    .to_string(),
                )),
                other => return Err(NodePathError::UnknownKind(other.to_string())),
            }
            i += 2;
        }
        Ok(Self(out))
    }

    /// Structural validation: must target at least one concrete segment.
    pub fn is_structurally_valid(&self) -> bool {
        self.0.iter().any(|seg| {
            matches!(
                seg,
                NodePathSegment::Engine(_)
                    | NodePathSegment::Layer(_)
                    | NodePathSegment::Module(_)
                    | NodePathSegment::Block(_)
                    | NodePathSegment::Parameter(_)
            )
        })
    }

    pub fn validate(&self) -> Result<(), NodePathError> {
        if self.0.is_empty() {
            return Err(NodePathError::Empty);
        }
        if self.is_structurally_valid() {
            Ok(())
        } else {
            Err(NodePathError::UnknownKind("raw".to_string()))
        }
    }
}

impl From<&str> for NodePath {
    fn from(s: &str) -> Self {
        Self::parse_legacy(s)
    }
}

// ─── Override operation ─────────────────────────────────────────

/// What to do at the override target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
#[repr(C)]
pub enum NodeOverrideOp {
    /// Set parameter to an absolute value.
    Set(ParameterValue),
    /// Bypass a block or module.
    Bypass(bool),
    /// Replace a referenced variant/preset id at a path.
    ReplaceRef(String),
    /// Insert a node/reference before the targeted path.
    InsertBefore(String),
    /// Insert a node/reference after the targeted path.
    InsertAfter(String),
    /// Remove the targeted node/reference.
    Remove,
    /// Toggle enable/disable semantics.
    Enable(bool),
}

// ─── Override entry ─────────────────────────────────────────────

/// A single override: path + operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Facet)]
pub struct Override {
    pub path: NodePath,
    pub op: NodeOverrideOp,
}

impl Override {
    pub fn set(path: impl Into<NodePath>, value: f32) -> Self {
        Self {
            path: path.into(),
            op: NodeOverrideOp::Set(ParameterValue::new(value)),
        }
    }

    pub fn bypass(path: impl Into<NodePath>, bypassed: bool) -> Self {
        Self {
            path: path.into(),
            op: NodeOverrideOp::Bypass(bypassed),
        }
    }
}

// Backward-compatible names while call sites migrate.
pub type OverridePath = NodePath;
pub type OverrideOp = NodeOverrideOp;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_override_path_segments() {
        let path = OverridePath::from("module.drive.block.od.param.gain");
        assert!(path.is_structurally_valid());
        assert_eq!(path.as_str(), "module.drive.block.od.param.gain");
        assert_eq!(path.segments().len(), 3);
    }

    #[test]
    fn test_override_set() {
        let ov = Override::set("module.eq.param.freq", 0.75);
        assert_eq!(ov.path.as_str(), "module.eq.param.freq");
        match &ov.op {
            OverrideOp::Set(v) => assert!((v.get() - 0.75).abs() < f32::EPSILON),
            _ => panic!("expected Set"),
        }
    }

    #[test]
    fn test_builder_path() {
        let path = NodePath::module("drive-duo")
            .with_block("drive-1")
            .with_parameter("gain");
        assert_eq!(path.as_str(), "module.drive-duo.block.drive-1.param.gain");
        assert!(path.validate().is_ok());
    }

    #[test]
    fn test_try_parse_strict_rejects_unknown_kind() {
        let err = NodePath::try_parse_legacy("foo.bar").unwrap_err();
        assert!(matches!(err, NodePathError::UnknownKind(_)));
    }
}

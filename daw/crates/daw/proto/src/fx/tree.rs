//! FX Tree Model — recursive container-aware FX chain representation.
//!
//! This module provides the types for representing REAPER's FX chain hierarchy,
//! including nested containers with serial/parallel routing modes. The tree model
//! abstracts away REAPER's stride-based raw index encoding behind stable `FxNodeId`
//! identifiers.
//!
//! # Key Types
//!
//! - [`FxNodeId`] — Stable identifier for any node in the FX tree
//! - [`FxNode`] — A node in the tree (either a plugin or a container)
//! - [`FxNodeKind`] — Whether the node is a plugin wrapping an [`Fx`] or a container
//! - [`FxRoutingMode`] — Serial or parallel routing within a container
//! - [`FxContainerChannelConfig`] — Channel count configuration for a container
//! - [`FxTree`] — The complete FX chain as an ordered list of top-level nodes

use super::types::Fx;
use facet::Facet;

// =============================================================================
// FxNodeId — stable node identifier
// =============================================================================

/// Stable identifier for a node in the FX tree.
///
/// Abstracts away REAPER's raw stride-based `0x2000000 + offset` encoding.
/// For plugins, this is typically the FX GUID. For containers, this is a
/// synthetic path-based identifier since REAPER doesn't assign GUIDs to
/// container slots themselves.
///
/// The internal representation is an opaque string so the encoding strategy
/// can evolve without breaking API consumers.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Facet)]
pub struct FxNodeId(pub String);

impl FxNodeId {
    /// Create an `FxNodeId` from an FX GUID (for plugin nodes).
    pub fn from_guid(guid: impl Into<String>) -> Self {
        Self(guid.into())
    }

    /// Create a synthetic `FxNodeId` for a container at the given path.
    ///
    /// The path encodes the container's position in the tree. For example,
    /// a container at top-level index 2 would be `"container:2"`, and a
    /// nested container at position 1 inside that would be `"container:2:1"`.
    pub fn container(path: impl Into<String>) -> Self {
        Self(format!("container:{}", path.into()))
    }

    /// Returns the raw identifier string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns `true` if this is a container identifier.
    pub fn is_container(&self) -> bool {
        self.0.starts_with("container:")
    }
}

impl std::fmt::Display for FxNodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for FxNodeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for FxNodeId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

// =============================================================================
// FxRoutingMode — serial vs parallel
// =============================================================================

/// Routing mode for FX within a container.
///
/// REAPER containers can process their children in serial (one after another)
/// or parallel (all receive the same input, outputs are summed).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Facet)]
pub enum FxRoutingMode {
    /// FX are processed in sequence (output of one feeds the next).
    #[default]
    Serial,
    /// FX are processed in parallel (all receive the same input, outputs summed).
    Parallel,
}

impl FxRoutingMode {
    /// Parse from REAPER's `GetNamedConfigParm(fx, "parallel")` value.
    ///
    /// REAPER returns `"0"` for serial and `"1"` for parallel.
    pub fn from_reaper_param(value: &str) -> Self {
        match value.trim() {
            "1" => Self::Parallel,
            _ => Self::Serial,
        }
    }

    /// Convert to REAPER's `SetNamedConfigParm` value.
    pub fn to_reaper_param(self) -> &'static str {
        match self {
            Self::Serial => "0",
            Self::Parallel => "1",
        }
    }
}

// =============================================================================
// FxContainerChannelConfig — channel routing
// =============================================================================

/// Channel count configuration for an FX container.
///
/// REAPER containers can have independent input, processing, and output
/// channel counts, allowing for complex routing (e.g., stereo in,
/// 8-channel internal processing for parallel lanes, stereo out).
#[derive(Clone, Debug, Default, PartialEq, Eq, Facet)]
pub struct FxContainerChannelConfig {
    /// Number of channels inside the container (processing channels).
    pub nch: u32,
    /// Number of input channels to the container.
    pub nch_in: u32,
    /// Number of output channels from the container.
    pub nch_out: u32,
}

impl FxContainerChannelConfig {
    /// Create a stereo (2-channel) configuration.
    pub fn stereo() -> Self {
        Self {
            nch: 2,
            nch_in: 2,
            nch_out: 2,
        }
    }

    /// Silent configuration — zero output channels for gapless container switching.
    ///
    /// Sets `nch_out = 0` to suppress all output routing from the container
    /// without affecting its internal processing or input channels.
    pub fn silent() -> Self {
        Self {
            nch: 0,
            nch_in: 0,
            nch_out: 0,
        }
    }
}

// =============================================================================
// FxNodeKind — plugin vs container
// =============================================================================

/// The kind of node in the FX tree.
///
/// A node is either a single plugin (wrapping the existing [`Fx`] struct)
/// or a container that holds child nodes with a routing mode.
#[repr(C)]
#[derive(Clone, Debug, Facet)]
pub enum FxNodeKind {
    /// A single FX plugin instance.
    Plugin(Fx),

    /// A container that holds child FX nodes.
    Container {
        /// Display name of the container (e.g., "DRIVE", "PRE-FX", "AMP").
        name: String,
        /// Child nodes within this container.
        children: Vec<FxNode>,
        /// Routing mode (serial or parallel).
        routing: FxRoutingMode,
        /// Channel configuration.
        channel_config: FxContainerChannelConfig,
    },
}

impl FxNodeKind {
    /// Returns `true` if this is a container.
    pub fn is_container(&self) -> bool {
        matches!(self, Self::Container { .. })
    }

    /// Returns `true` if this is a plugin.
    pub fn is_plugin(&self) -> bool {
        matches!(self, Self::Plugin(_))
    }

    /// Returns the display name for this node.
    pub fn display_name(&self) -> &str {
        match self {
            Self::Plugin(fx) => &fx.name,
            Self::Container { name, .. } => name,
        }
    }
}

// =============================================================================
// FxNode — a single node in the tree
// =============================================================================

/// A node in the FX chain tree.
///
/// Each node has a stable [`FxNodeId`], a kind (plugin or container),
/// enabled state, and a reference to its parent container (if any).
#[derive(Clone, Debug, Facet)]
pub struct FxNode {
    /// Stable identifier for this node.
    pub id: FxNodeId,
    /// What kind of node this is.
    pub kind: FxNodeKind,
    /// Whether this node is enabled (not bypassed).
    pub enabled: bool,
    /// Parent container's ID, or `None` if at the top level.
    pub parent_id: Option<FxNodeId>,
}

impl FxNode {
    /// Create a new plugin node.
    pub fn plugin(id: FxNodeId, fx: Fx, enabled: bool, parent_id: Option<FxNodeId>) -> Self {
        Self {
            id,
            kind: FxNodeKind::Plugin(fx),
            enabled,
            parent_id,
        }
    }

    /// Create a new container node.
    pub fn container(
        id: FxNodeId,
        name: impl Into<String>,
        routing: FxRoutingMode,
        channel_config: FxContainerChannelConfig,
        enabled: bool,
        parent_id: Option<FxNodeId>,
    ) -> Self {
        Self {
            id,
            kind: FxNodeKind::Container {
                name: name.into(),
                children: Vec::new(),
                routing,
                channel_config,
            },
            enabled,
            parent_id,
        }
    }

    /// Returns the display name of this node.
    pub fn display_name(&self) -> &str {
        self.kind.display_name()
    }

    /// Returns `true` if this node is a container.
    pub fn is_container(&self) -> bool {
        self.kind.is_container()
    }

    /// Returns `true` if this node is a plugin.
    pub fn is_plugin(&self) -> bool {
        self.kind.is_plugin()
    }

    /// Returns the children if this is a container, or an empty slice.
    pub fn children(&self) -> &[FxNode] {
        match &self.kind {
            FxNodeKind::Container { children, .. } => children,
            FxNodeKind::Plugin(_) => &[],
        }
    }

    /// Returns a mutable reference to children if this is a container.
    pub fn children_mut(&mut self) -> Option<&mut Vec<FxNode>> {
        match &mut self.kind {
            FxNodeKind::Container { children, .. } => Some(children),
            FxNodeKind::Plugin(_) => None,
        }
    }

    /// Returns the FX info if this is a plugin node.
    pub fn as_plugin(&self) -> Option<&Fx> {
        match &self.kind {
            FxNodeKind::Plugin(fx) => Some(fx),
            FxNodeKind::Container { .. } => None,
        }
    }
}

// =============================================================================
// FxTree — the complete FX chain hierarchy
// =============================================================================

/// The complete FX chain represented as a tree.
///
/// Contains an ordered list of top-level nodes. Containers hold their
/// children inline, forming a recursive structure that mirrors REAPER's
/// actual FX chain with all container nesting.
#[derive(Clone, Debug, Default, Facet)]
pub struct FxTree {
    /// Top-level nodes in the FX chain (in order).
    pub nodes: Vec<FxNode>,
}

impl FxTree {
    /// Create an empty FX tree.
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    /// Create a tree from a list of top-level nodes.
    pub fn from_nodes(nodes: Vec<FxNode>) -> Self {
        Self { nodes }
    }

    /// Returns `true` if the tree has no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns the total number of nodes (including nested children).
    pub fn total_count(&self) -> usize {
        fn count_recursive(nodes: &[FxNode]) -> usize {
            nodes
                .iter()
                .map(|n| 1 + count_recursive(n.children()))
                .sum()
        }
        count_recursive(&self.nodes)
    }

    /// Iterate over all top-level nodes.
    pub fn iter(&self) -> std::slice::Iter<'_, FxNode> {
        self.nodes.iter()
    }

    /// Depth-first iterator over all nodes in the tree.
    ///
    /// Yields `(depth, &FxNode)` pairs where depth 0 = top-level.
    pub fn iter_depth_first(&self) -> FxTreeDepthFirstIter<'_> {
        FxTreeDepthFirstIter {
            stack: self.nodes.iter().rev().map(|n| (0, n)).collect(),
        }
    }

    /// Find a node by its ID (searches recursively).
    pub fn find_node(&self, id: &FxNodeId) -> Option<&FxNode> {
        fn find_recursive<'a>(nodes: &'a [FxNode], id: &FxNodeId) -> Option<&'a FxNode> {
            for node in nodes {
                if &node.id == id {
                    return Some(node);
                }
                if let Some(found) = find_recursive(node.children(), id) {
                    return Some(found);
                }
            }
            None
        }
        find_recursive(&self.nodes, id)
    }

    /// Find a plugin node by FX GUID (searches recursively).
    pub fn find_by_guid(&self, guid: &str) -> Option<&FxNode> {
        fn find_recursive<'a>(nodes: &'a [FxNode], guid: &str) -> Option<&'a FxNode> {
            for node in nodes {
                if let FxNodeKind::Plugin(fx) = &node.kind
                    && fx.guid == guid
                {
                    return Some(node);
                }
                if let Some(found) = find_recursive(node.children(), guid) {
                    return Some(found);
                }
            }
            None
        }
        find_recursive(&self.nodes, guid)
    }

    /// Find a mutable reference to a node by its ID.
    pub fn find_node_mut(&mut self, id: &FxNodeId) -> Option<&mut FxNode> {
        fn find_recursive<'a>(nodes: &'a mut [FxNode], id: &FxNodeId) -> Option<&'a mut FxNode> {
            // Two-pass to satisfy the borrow checker: first check if current node matches,
            // then recurse into children.
            let idx = nodes.iter().position(|n| &n.id == id);
            if let Some(idx) = idx {
                return Some(&mut nodes[idx]);
            }
            for node in nodes.iter_mut() {
                if let Some(children) = node.children_mut()
                    && let Some(found) = find_recursive(children, id)
                {
                    return Some(found);
                }
            }
            None
        }
        find_recursive(&mut self.nodes, id)
    }

    /// Get the nesting depth of a node (0 = top-level).
    pub fn depth_of(&self, id: &FxNodeId) -> Option<usize> {
        fn find_depth(nodes: &[FxNode], id: &FxNodeId, current_depth: usize) -> Option<usize> {
            for node in nodes {
                if &node.id == id {
                    return Some(current_depth);
                }
                if let Some(depth) = find_depth(node.children(), id, current_depth + 1) {
                    return Some(depth);
                }
            }
            None
        }
        find_depth(&self.nodes, id, 0)
    }

    /// Add a node to the top level of the tree.
    pub fn push(&mut self, node: FxNode) {
        self.nodes.push(node);
    }
}

// =============================================================================
// Depth-first iterator
// =============================================================================

/// Depth-first iterator over all nodes in an [`FxTree`].
///
/// Yields `(depth, &FxNode)` pairs.
pub struct FxTreeDepthFirstIter<'a> {
    stack: Vec<(usize, &'a FxNode)>,
}

impl<'a> Iterator for FxTreeDepthFirstIter<'a> {
    type Item = (usize, &'a FxNode);

    fn next(&mut self) -> Option<Self::Item> {
        let (depth, node) = self.stack.pop()?;

        // Push children in reverse order so they're visited left-to-right
        for child in node.children().iter().rev() {
            self.stack.push((depth + 1, child));
        }

        Some((depth, node))
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a simple plugin Fx for testing.
    fn test_fx(name: &str, guid: &str) -> Fx {
        Fx {
            guid: guid.to_string(),
            index: 0,
            name: name.to_string(),
            plugin_name: name.to_string(),
            ..Default::default()
        }
    }

    /// Build a sample tree:
    /// ```text
    /// [0] ReaEQ (plugin)
    /// [1] DRIVE (container, serial)
    ///     [1.0] TubeScreamer (plugin)
    ///     [1.1] Distortion (plugin)
    /// [2] AMP (container, parallel)
    ///     [2.0] AmpSim1 (plugin)
    ///     [2.1] CABINET (container, serial)
    ///         [2.1.0] CabSim (plugin)
    /// [3] ReaComp (plugin)
    /// ```
    fn build_sample_tree() -> FxTree {
        let reaeq = FxNode::plugin(
            FxNodeId::from_guid("guid-reaeq"),
            test_fx("ReaEQ", "guid-reaeq"),
            true,
            None,
        );

        let mut drive = FxNode::container(
            FxNodeId::container("1"),
            "DRIVE",
            FxRoutingMode::Serial,
            FxContainerChannelConfig::stereo(),
            true,
            None,
        );
        drive.children_mut().unwrap().push(FxNode::plugin(
            FxNodeId::from_guid("guid-ts"),
            test_fx("TubeScreamer", "guid-ts"),
            true,
            Some(FxNodeId::container("1")),
        ));
        drive.children_mut().unwrap().push(FxNode::plugin(
            FxNodeId::from_guid("guid-dist"),
            test_fx("Distortion", "guid-dist"),
            false,
            Some(FxNodeId::container("1")),
        ));

        let mut cabinet = FxNode::container(
            FxNodeId::container("2:1"),
            "CABINET",
            FxRoutingMode::Serial,
            FxContainerChannelConfig::stereo(),
            true,
            Some(FxNodeId::container("2")),
        );
        cabinet.children_mut().unwrap().push(FxNode::plugin(
            FxNodeId::from_guid("guid-cab"),
            test_fx("CabSim", "guid-cab"),
            true,
            Some(FxNodeId::container("2:1")),
        ));

        let mut amp = FxNode::container(
            FxNodeId::container("2"),
            "AMP",
            FxRoutingMode::Parallel,
            FxContainerChannelConfig {
                nch: 8,
                nch_in: 2,
                nch_out: 2,
            },
            true,
            None,
        );
        amp.children_mut().unwrap().push(FxNode::plugin(
            FxNodeId::from_guid("guid-amp1"),
            test_fx("AmpSim1", "guid-amp1"),
            true,
            Some(FxNodeId::container("2")),
        ));
        amp.children_mut().unwrap().push(cabinet);

        let reacomp = FxNode::plugin(
            FxNodeId::from_guid("guid-reacomp"),
            test_fx("ReaComp", "guid-reacomp"),
            true,
            None,
        );

        FxTree::from_nodes(vec![reaeq, drive, amp, reacomp])
    }

    #[test]
    fn test_total_count() {
        let tree = build_sample_tree();
        // ReaEQ + DRIVE(TS, Dist) + AMP(AmpSim1, CABINET(CabSim)) + ReaComp
        // = 1 + (1+2) + (1+1+(1+1)) + 1 = 9
        assert_eq!(tree.total_count(), 9);
    }

    #[test]
    fn test_find_node_top_level() {
        let tree = build_sample_tree();
        let node = tree.find_node(&FxNodeId::from_guid("guid-reaeq")).unwrap();
        assert_eq!(node.display_name(), "ReaEQ");
        assert!(node.is_plugin());
    }

    #[test]
    fn test_find_node_nested() {
        let tree = build_sample_tree();

        // Find plugin inside DRIVE container
        let ts = tree.find_node(&FxNodeId::from_guid("guid-ts")).unwrap();
        assert_eq!(ts.display_name(), "TubeScreamer");
        assert_eq!(ts.parent_id, Some(FxNodeId::container("1")));

        // Find plugin inside doubly-nested CABINET container
        let cab = tree.find_node(&FxNodeId::from_guid("guid-cab")).unwrap();
        assert_eq!(cab.display_name(), "CabSim");
        assert_eq!(cab.parent_id, Some(FxNodeId::container("2:1")));
    }

    #[test]
    fn test_find_container() {
        let tree = build_sample_tree();
        let drive = tree.find_node(&FxNodeId::container("1")).unwrap();
        assert!(drive.is_container());
        assert_eq!(drive.display_name(), "DRIVE");
        assert_eq!(drive.children().len(), 2);
    }

    #[test]
    fn test_find_by_guid() {
        let tree = build_sample_tree();
        let node = tree.find_by_guid("guid-dist").unwrap();
        assert_eq!(node.display_name(), "Distortion");
        assert!(!node.enabled); // Distortion is disabled in our sample
    }

    #[test]
    fn test_find_by_guid_not_found() {
        let tree = build_sample_tree();
        assert!(tree.find_by_guid("guid-nonexistent").is_none());
    }

    #[test]
    fn test_depth_of() {
        let tree = build_sample_tree();

        // Top-level plugin
        assert_eq!(tree.depth_of(&FxNodeId::from_guid("guid-reaeq")), Some(0));

        // Top-level container
        assert_eq!(tree.depth_of(&FxNodeId::container("1")), Some(0));

        // Plugin inside DRIVE container (depth 1)
        assert_eq!(tree.depth_of(&FxNodeId::from_guid("guid-ts")), Some(1));

        // CABINET inside AMP (depth 1)
        assert_eq!(tree.depth_of(&FxNodeId::container("2:1")), Some(1));

        // Plugin inside CABINET inside AMP (depth 2)
        assert_eq!(tree.depth_of(&FxNodeId::from_guid("guid-cab")), Some(2));
    }

    #[test]
    fn test_depth_first_iteration() {
        let tree = build_sample_tree();
        let items: Vec<(usize, &str)> = tree
            .iter_depth_first()
            .map(|(depth, node)| (depth, node.display_name()))
            .collect();

        assert_eq!(
            items,
            vec![
                (0, "ReaEQ"),
                (0, "DRIVE"),
                (1, "TubeScreamer"),
                (1, "Distortion"),
                (0, "AMP"),
                (1, "AmpSim1"),
                (1, "CABINET"),
                (2, "CabSim"),
                (0, "ReaComp"),
            ]
        );
    }

    #[test]
    fn test_routing_mode() {
        let tree = build_sample_tree();

        let drive = tree.find_node(&FxNodeId::container("1")).unwrap();
        if let FxNodeKind::Container { routing, .. } = &drive.kind {
            assert_eq!(*routing, FxRoutingMode::Serial);
        } else {
            panic!("Expected container");
        }

        let amp = tree.find_node(&FxNodeId::container("2")).unwrap();
        if let FxNodeKind::Container { routing, .. } = &amp.kind {
            assert_eq!(*routing, FxRoutingMode::Parallel);
        } else {
            panic!("Expected container");
        }
    }

    #[test]
    fn test_channel_config() {
        let tree = build_sample_tree();
        let amp = tree.find_node(&FxNodeId::container("2")).unwrap();
        if let FxNodeKind::Container { channel_config, .. } = &amp.kind {
            assert_eq!(channel_config.nch, 8);
            assert_eq!(channel_config.nch_in, 2);
            assert_eq!(channel_config.nch_out, 2);
        } else {
            panic!("Expected container");
        }
    }

    #[test]
    fn test_reaper_routing_mode_parsing() {
        assert_eq!(FxRoutingMode::from_reaper_param("0"), FxRoutingMode::Serial);
        assert_eq!(
            FxRoutingMode::from_reaper_param("1"),
            FxRoutingMode::Parallel
        );
        assert_eq!(
            FxRoutingMode::from_reaper_param(" 1 "),
            FxRoutingMode::Parallel
        );
        assert_eq!(FxRoutingMode::from_reaper_param(""), FxRoutingMode::Serial);
    }

    #[test]
    fn test_empty_tree() {
        let tree = FxTree::new();
        assert!(tree.is_empty());
        assert_eq!(tree.total_count(), 0);
        assert!(tree.find_node(&FxNodeId::from_guid("anything")).is_none());
        assert_eq!(tree.iter_depth_first().count(), 0);
    }

    #[test]
    fn test_find_node_mut() {
        let mut tree = build_sample_tree();

        // Disable ReaEQ
        let node = tree
            .find_node_mut(&FxNodeId::from_guid("guid-reaeq"))
            .unwrap();
        node.enabled = false;

        // Verify the change persists
        let node = tree.find_node(&FxNodeId::from_guid("guid-reaeq")).unwrap();
        assert!(!node.enabled);
    }

    #[test]
    fn test_node_id_is_container() {
        assert!(FxNodeId::container("1").is_container());
        assert!(FxNodeId::container("2:1").is_container());
        assert!(!FxNodeId::from_guid("some-guid").is_container());
    }
}
